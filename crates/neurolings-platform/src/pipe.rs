//! 本地 IPC 的命名管道 JSON 行传输（Windows）。
//! 管道名约定：\\.\pipe\<endpoint>。

#[cfg(windows)]
mod win {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE,
        FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FlushFileBuffers,
        OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, NAMED_PIPE_MODE, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
        PeekNamedPipe,
    };
    use windows::core::PCWSTR;

    use crate::{PlatformError, PlatformResult};

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn last_err(context: &str) -> PlatformError {
        PlatformError::Win32(format!("{context}: {}", std::io::Error::last_os_error()))
    }

    /// 命名管道服务端。每个连接占用一个管道实例：
    /// `pending` 为等待客户端的实例，accept 成功后另建新实例等待下一个客户端。
    pub struct PipeServer {
        name: Vec<u16>,
        pending: HANDLE,
    }

    impl PipeServer {
        /// 绑定管道。其他进程已持有时失败
        /// （FILE_FLAG_FIRST_PIPE_INSTANCE），用于单实例检查。
        pub fn bind(endpoint: &str) -> PlatformResult<Self> {
            let name = wide(&format!("\\\\.\\pipe\\{endpoint}"));
            let pending = create_instance(&name, true)?;
            Ok(Self { name, pending })
        }

        /// 阻塞直至客户端连接，返回该连接独占的管道实例句柄
        /// （由调用方关闭，见 IpcConnection 的 Drop）。
        pub fn accept(&mut self) -> PlatformResult<HANDLE> {
            let result = unsafe { ConnectNamedPipe(self.pending, None) };
            if result.is_err() {
                // 客户端先于 ConnectNamedPipe 连接时返回 ERROR_PIPE_CONNECTED，
                // 按 MSDN 此时管道已连接，视为成功。
                const ERROR_PIPE_CONNECTED: i32 = 231;
                if std::io::Error::last_os_error().raw_os_error() != Some(ERROR_PIPE_CONNECTED) {
                    return Err(last_err("ConnectNamedPipe"));
                }
            }
            // 为下一个客户端预先创建新实例，当前实例交给连接处理线程
            let next = create_instance(&self.name, false)?;
            Ok(std::mem::replace(&mut self.pending, next))
        }
    }

    /// 创建一个管道实例；`first` 时携带 FILE_FLAG_FIRST_PIPE_INSTANCE。
    fn create_instance(name: &[u16], first: bool) -> PlatformResult<HANDLE> {
        let mut open_mode = PIPE_ACCESS_DUPLEX.0;
        if first {
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE.0;
        }
        let pipe_mode = NAMED_PIPE_MODE(
            PIPE_TYPE_BYTE.0 | PIPE_READMODE_BYTE.0 | PIPE_WAIT.0 | PIPE_REJECT_REMOTE_CLIENTS.0,
        );
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(open_mode),
                pipe_mode,
                PIPE_UNLIMITED_INSTANCES,
                65536,
                65536,
                0,
                None,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(last_err("CreateNamedPipeW"));
        }
        Ok(handle)
    }

    /// 从管道句柄读取一行（以换行结尾，带长度上限与总读超时）。
    ///
    /// 用 PeekNamedPipe 轮询代替裸 ReadFile：有数据才读（单字节读取不会
    /// 再阻塞），无数据 sleep 10ms 重试，累计超过 timeout 判定超时断开。
    /// 参考实现：ShijimaLocalApi.cc 的 readMessage 使用 waitForReadyRead(2000)。
    pub fn read_handle_line(
        handle: HANDLE,
        max_bytes: usize,
        timeout: Duration,
    ) -> PlatformResult<Option<String>> {
        let deadline = Instant::now() + timeout;
        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let mut available = 0u32;
            let peeked =
                unsafe { PeekNamedPipe(handle, None, 0, None, Some(&mut available), None) };
            if peeked.is_err() {
                // 管道断开：按客户端断开处理
                if buffer.is_empty() {
                    return Ok(None);
                }
                break;
            }
            if available == 0 {
                if Instant::now() >= deadline {
                    // 读超时：与断开同样处理，交还已读部分
                    if buffer.is_empty() {
                        return Ok(None);
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            let mut read = 0u32;
            let ok = unsafe { ReadFile(handle, Some(&mut byte), Some(&mut read), None) };
            if ok.is_err() || read == 0 {
                if buffer.is_empty() {
                    return Ok(None);
                }
                break;
            }
            if byte[0] == b'\n' {
                break;
            }
            buffer.push(byte[0]);
            if buffer.len() > max_bytes {
                return Err(PlatformError::Win32("IPC message too large".into()));
            }
        }
        Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
    }

    pub fn write_handle_line(handle: HANDLE, line: &str) -> PlatformResult<()> {
        let mut payload = line.as_bytes().to_vec();
        payload.push(b'\n');
        let mut written = 0u32;
        unsafe { WriteFile(handle, Some(&payload), Some(&mut written), None) }
            .map_err(|_| last_err("WriteFile"))?;
        if written as usize != payload.len() {
            return Err(PlatformError::Win32(
                "IPC request was only partially written".into(),
            ));
        }
        unsafe { FlushFileBuffers(handle) }.map_err(|_| last_err("FlushFileBuffers"))
    }

    impl Drop for PipeServer {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.pending);
            }
        }
    }

    /// 使用同一超时完成管道连接和请求/响应交换。
    pub fn pipe_client_call(
        endpoint: &str,
        request_line: &str,
        timeout: Duration,
        max_bytes: usize,
    ) -> PlatformResult<String> {
        pipe_client_call_with_timeouts(endpoint, request_line, timeout, timeout, max_bytes)
    }

    /// 使用独立的连接和读取超时完成管道请求/响应交换。
    pub fn pipe_client_call_with_timeouts(
        endpoint: &str,
        request_line: &str,
        connect_timeout: Duration,
        read_timeout: Duration,
        max_bytes: usize,
    ) -> PlatformResult<String> {
        let name = wide(&format!("\\\\.\\pipe\\{endpoint}"));
        let connect_deadline = Instant::now() + connect_timeout;
        let handle = loop {
            let access = FILE_GENERIC_READ | FILE_GENERIC_WRITE;
            let result = unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    access.0,
                    windows::Win32::Storage::FileSystem::FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(FILE_ATTRIBUTE_NORMAL.0),
                    None,
                )
            };
            match result {
                Ok(handle) => break handle,
                Err(err) => {
                    const ERROR_PIPE_BUSY_HRESULT: i32 = -2147024665;
                    if err.code().0 == ERROR_PIPE_BUSY_HRESULT {
                        if Instant::now() >= connect_deadline {
                            return Err(PlatformError::Win32("timed out waiting for IPC".into()));
                        }
                        std::thread::sleep(Duration::from_millis(25));
                        continue;
                    }
                    return Err(PlatformError::Win32(format!("IPC connect: {err}")));
                }
            }
        };

        let mut payload = request_line.as_bytes().to_vec();
        payload.push(b'\n');
        let mut written = 0u32;
        if unsafe { WriteFile(handle, Some(&payload), Some(&mut written), None) }.is_err() {
            let error = last_err("WriteFile");
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(error);
        }
        if written as usize != payload.len() {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(PlatformError::Win32(
                "IPC request was only partially written".into(),
            ));
        }

        let read_deadline = Instant::now() + read_timeout;
        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            // PeekNamedPipe 轮询代替裸 ReadFile：后者会无限阻塞，使
            // deadline 形同虚设。有数据才读（单字节读取不会再阻塞），
            // 无数据 sleep 10ms 重试，总时限仍为 deadline。
            let mut available = 0u32;
            let peeked =
                unsafe { PeekNamedPipe(handle, None, 0, None, Some(&mut available), None) };
            if peeked.is_err() {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(PlatformError::Win32(
                    "IPC connection closed before a complete response was received".into(),
                ));
            }
            if available == 0 {
                if Instant::now() >= read_deadline {
                    unsafe {
                        let _ = CloseHandle(handle);
                    }
                    return Err(PlatformError::Win32(
                        "Timed out waiting for IPC response".into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            let mut read = 0u32;
            let ok = unsafe { ReadFile(handle, Some(&mut byte), Some(&mut read), None) };
            if ok.is_err() || read == 0 {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(PlatformError::Win32(
                    "IPC connection closed before a complete response was received".into(),
                ));
            }
            if byte[0] == b'\n' {
                break;
            }
            buffer.push(byte[0]);
            if buffer.len() > max_bytes {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(PlatformError::Win32(
                    "IPC message exceeds the maximum size".into(),
                ));
            }
        }
        unsafe {
            let _ = CloseHandle(handle);
        }
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::mpsc;

        #[test]
        fn client_read_timeout_starts_after_connection() {
            let endpoint = format!("neurolings-ipc-timeout-test-{}", std::process::id());
            let server_endpoint = endpoint.clone();
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let server = std::thread::spawn(move || {
                let mut server = PipeServer::bind(&server_endpoint).expect("bind test pipe");
                ready_tx.send(()).expect("signal test pipe readiness");
                let handle = server.accept().expect("accept test client");
                let request =
                    read_handle_line(handle, 1024, Duration::from_secs(1)).expect("read request");
                assert_eq!(request.as_deref(), Some(r#"{"command":"ping"}"#));
                std::thread::sleep(Duration::from_millis(80));
                write_handle_line(handle, r#"{"ok":true}"#).expect("write response");
                unsafe {
                    let _ = CloseHandle(handle);
                }
            });

            ready_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("wait for test pipe");
            let response = pipe_client_call_with_timeouts(
                &endpoint,
                r#"{"command":"ping"}"#,
                Duration::from_millis(20),
                Duration::from_secs(1),
                1024,
            )
            .expect("read timeout must not consume connect timeout");
            assert_eq!(response, r#"{"ok":true}"#);
            server.join().expect("join test pipe");
        }
    }
}

#[cfg(windows)]
pub use win::{
    PipeServer, pipe_client_call, pipe_client_call_with_timeouts, read_handle_line,
    write_handle_line,
};
