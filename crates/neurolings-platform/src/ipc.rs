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
    pub fn accept_client(&mut self) -> PlatformResult<IpcConnection> {
        #[cfg(windows)]
        {
            // 每次 accept 得到一个独立的管道实例句柄，由 IpcConnection 持有并关闭
            let handle = self.pipe.accept()?;
            Ok(IpcConnection { handle })
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
}

pub struct IpcConnection {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    stream: Option<std::os::unix::net::UnixStream>,
}

// Windows 的 HANDLE 非 Send；连接句柄由处理线程独占持有并负责关闭，
// 服务端通过另建管道实例接受后续连接（见 pipe.rs PipeServer::accept）。
unsafe impl Send for IpcConnection {}

impl Drop for IpcConnection {
    fn drop(&mut self) {
        // Windows：连接独占管道实例句柄，处理完毕即关闭（替代原来的
        // DisconnectNamedPipe 复位，使 accept 循环与连接处理可并行）。
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
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

/// 服务端读单条请求的总超时为 2000ms
/// （ShijimaLocalApi.cc:122 readMessage 的 2000ms）。
#[cfg(windows)]
const IPC_READ_TIMEOUT: Duration = Duration::from_millis(2000);

impl IpcConnection {
    pub fn read_line(&mut self, max_bytes: usize) -> PlatformResult<Option<String>> {
        #[cfg(windows)]
        {
            crate::pipe::read_handle_line(self.handle, max_bytes, IPC_READ_TIMEOUT)
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

/// 客户端一次性请求/响应，连接和读取共用同一超时。
pub fn ipc_client_call(
    endpoint: &str,
    request_line: &str,
    timeout: Duration,
    max_bytes: usize,
) -> PlatformResult<String> {
    ipc_client_call_with_timeouts(endpoint, request_line, timeout, timeout, max_bytes)
}

/// 客户端一次性请求/响应，分别限制连接与读取阶段。
pub fn ipc_client_call_with_timeouts(
    endpoint: &str,
    request_line: &str,
    connect_timeout: Duration,
    read_timeout: Duration,
    max_bytes: usize,
) -> PlatformResult<String> {
    #[cfg(windows)]
    {
        crate::pipe::pipe_client_call_with_timeouts(
            endpoint,
            request_line,
            connect_timeout,
            read_timeout,
            max_bytes,
        )
    }
    #[cfg(unix)]
    {
        use std::io::{Read, Write};
        let path = unix_socket_path(endpoint);
        let deadline = std::time::Instant::now() + connect_timeout;
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
        if !connect_timeout.is_zero() {
            stream.set_write_timeout(Some(connect_timeout)).ok();
        }
        let mut payload = request_line.as_bytes().to_vec();
        payload.push(b'\n');
        stream
            .write_all(&payload)
            .map_err(|e| crate::PlatformError::Win32(format!("write: {e}")))?;
        let nonblocking_read = read_timeout.is_zero();
        if nonblocking_read {
            stream.set_nonblocking(true).ok();
        } else {
            stream.set_read_timeout(Some(read_timeout)).ok();
        }
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
                Err(e) if nonblocking_read && e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err(crate::PlatformError::Win32(
                        "Timed out waiting for IPC response".into(),
                    ));
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
