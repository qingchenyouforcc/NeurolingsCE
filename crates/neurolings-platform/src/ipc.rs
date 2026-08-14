//! 跨平台本地 IPC 传输（JSON 行）。
//! Windows：命名管道 \\.\pipe\<endpoint>（见 pipe.rs）。
//! Unix：$XDG_RUNTIME_DIR/<endpoint>.sock（回退 /tmp）上的域套接字。

use std::time::Duration;

use crate::PlatformResult;

pub struct IpcServerTransport {
    #[cfg(windows)]
    pipe: crate::pipe::PipeServer,
    #[cfg(unix)]
    listener: std::os::unix::net::UnixListener,
    #[cfg(unix)]
    socket_path: std::path::PathBuf,
}

#[cfg(unix)]
fn unix_socket_path(endpoint: &str) -> std::path::PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
    dir.join(format!("{endpoint}.sock"))
}

impl IpcServerTransport {
    /// 绑定传输端点。其他进程已持有时失败，用作单实例守卫。
    pub fn bind(endpoint: &str) -> PlatformResult<Self> {
        #[cfg(windows)]
        {
            Ok(Self {
                pipe: crate::pipe::PipeServer::bind(endpoint)?,
            })
        }
        #[cfg(unix)]
        {
            use std::os::unix::net::UnixStream;
            let path = unix_socket_path(endpoint);
            // 旧套接字仍可接受连接时，说明端点由其他运行时持有。
            if path.exists() && UnixStream::connect(&path).is_ok() {
                return Err(crate::PlatformError::Win32(
                    "IPC endpoint already owned".into(),
                ));
            }
            let _ = std::fs::remove_file(&path);
            let listener = std::os::unix::net::UnixListener::bind(&path)
                .map_err(|e| crate::PlatformError::Win32(format!("bind: {e}")))?;
            Ok(Self {
                listener,
                socket_path: path,
            })
        }
    }

    /// 接受一个客户端并返回其连接句柄。
    pub fn accept_client(&self) -> PlatformResult<IpcConnection> {
        #[cfg(windows)]
        {
            self.pipe.accept()?;
            Ok(IpcConnection {
                handle: self.pipe.raw_handle(),
            })
        }
        #[cfg(unix)]
        {
            let (stream, _) = self
                .listener
                .accept()
                .map_err(|e| crate::PlatformError::Win32(format!("accept: {e}")))?;
            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
            Ok(IpcConnection {
                stream: Some(stream),
            })
        }
    }

    /// 标记当前连接已处理（Windows 管道断开即复位，无需处理）。
    pub fn end_connection(&self) {
        #[cfg(windows)]
        self.pipe.end_connection();
    }
}

impl Drop for IpcServerTransport {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

pub struct IpcConnection {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    stream: Option<std::os::unix::net::UnixStream>,
}

impl IpcConnection {
    pub fn read_line(&mut self, max_bytes: usize) -> PlatformResult<Option<String>> {
        #[cfg(windows)]
        {
            crate::pipe::read_handle_line(self.handle, max_bytes)
        }
        #[cfg(unix)]
        {
            use std::io::Read;
            let Some(stream) = &mut self.stream else {
                return Ok(None);
            };
            let mut buffer = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                match stream.read(&mut byte) {
                    Ok(0) => {
                        if buffer.is_empty() {
                            return Ok(None);
                        }
                        break;
                    }
                    Ok(_) => {
                        if byte[0] == b'\n' {
                            break;
                        }
                        buffer.push(byte[0]);
                        if buffer.len() > max_bytes {
                            return Err(crate::PlatformError::Win32(
                                "IPC message too large".into(),
                            ));
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                        return Ok(if buffer.is_empty() {
                            None
                        } else {
                            Some(String::from_utf8_lossy(&buffer).into_owned())
                        });
                    }
                    Err(e) => {
                        return Err(crate::PlatformError::Win32(format!("read: {e}")));
                    }
                }
            }
            Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
        }
    }

    pub fn write_line(&mut self, line: &str) -> PlatformResult<()> {
        #[cfg(windows)]
        {
            crate::pipe::write_handle_line(self.handle, line)
        }
        #[cfg(unix)]
        {
            use std::io::Write;
            let Some(stream) = &mut self.stream else {
                return Err(crate::PlatformError::Win32("closed".into()));
            };
            let mut payload = line.as_bytes().to_vec();
            payload.push(b'\n');
            stream
                .write_all(&payload)
                .map_err(|e| crate::PlatformError::Win32(format!("write: {e}")))
        }
    }
}

/// 客户端一次性请求/响应。
pub fn ipc_client_call(
    endpoint: &str,
    request_line: &str,
    timeout: Duration,
    max_bytes: usize,
) -> PlatformResult<String> {
    #[cfg(windows)]
    {
        crate::pipe::pipe_client_call(endpoint, request_line, timeout, max_bytes)
    }
    #[cfg(unix)]
    {
        use std::io::{Read, Write};
        let path = unix_socket_path(endpoint);
        let deadline = std::time::Instant::now() + timeout;
        let mut stream = loop {
            match std::os::unix::net::UnixStream::connect(&path) {
                Ok(s) => break s,
                Err(e) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(crate::PlatformError::Win32(format!(
                            "Timed out waiting for IPC: {e}"
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        };
        stream.set_read_timeout(Some(timeout)).ok();
        let mut payload = request_line.as_bytes().to_vec();
        payload.push(b'\n');
        stream
            .write_all(&payload)
            .map_err(|e| crate::PlatformError::Win32(format!("write: {e}")))?;
        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match stream.read(&mut byte) {
                Ok(0) => {
                    return Err(crate::PlatformError::Win32(
                        "IPC connection closed before a complete response was received".into(),
                    ));
                }
                Ok(_) => {
                    if byte[0] == b'\n' {
                        break;
                    }
                    buffer.push(byte[0]);
                    if buffer.len() > max_bytes {
                        return Err(crate::PlatformError::Win32(
                            "IPC message exceeds the maximum size".into(),
                        ));
                    }
                }
                Err(e) => {
                    return Err(crate::PlatformError::Win32(format!(
                        "Timed out waiting for IPC response: {e}"
                    )));
                }
            }
        }
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    }
}
