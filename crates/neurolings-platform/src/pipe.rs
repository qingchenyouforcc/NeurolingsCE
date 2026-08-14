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
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, NAMED_PIPE_MODE,
        PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
        PIPE_WAIT,
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

    pub struct PipeServer {
        handle: HANDLE,
    }

    impl PipeServer {
        /// 绑定管道。其他进程已持有时失败
        /// （FILE_FLAG_FIRST_PIPE_INSTANCE），用于单实例检查。
        pub fn bind(endpoint: &str) -> PlatformResult<Self> {
            let name = wide(&format!("\\\\.\\pipe\\{endpoint}"));
            let open_mode =
                FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_DUPLEX.0 | FILE_FLAG_FIRST_PIPE_INSTANCE.0);
            let pipe_mode = NAMED_PIPE_MODE(
                PIPE_TYPE_BYTE.0
                    | PIPE_READMODE_BYTE.0
                    | PIPE_WAIT.0
                    | PIPE_REJECT_REMOTE_CLIENTS.0,
            );
            let handle = unsafe {
                CreateNamedPipeW(
                    PCWSTR(name.as_ptr()),
                    open_mode,
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
            Ok(Self { handle })
        }

        /// 阻塞直至客户端连接。
        pub fn accept(&self) -> PlatformResult<()> {
            unsafe { ConnectNamedPipe(self.handle, None) }.map_err(|_| last_err("ConnectNamedPipe"))
        }

        pub fn raw_handle(&self) -> HANDLE {
            self.handle
        }

        /// 读取一行（以换行结尾，带长度上限）。
        pub fn read_line(&self, max_bytes: usize) -> PlatformResult<Option<String>> {
            read_handle_line(self.handle, max_bytes)
        }

        pub fn write_line(&self, line: &str) -> PlatformResult<()> {
            write_handle_line(self.handle, line)
        }

        pub fn end_connection(&self) {
            let _ = unsafe { DisconnectNamedPipe(self.handle) };
        }
    }

    /// 从管道句柄读取一行（带长度上限）。
    pub fn read_handle_line(handle: HANDLE, max_bytes: usize) -> PlatformResult<Option<String>> {
        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];
        loop {
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
        unsafe { FlushFileBuffers(handle) }.map_err(|_| last_err("FlushFileBuffers"))
    }

    impl Drop for PipeServer {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }

    /// 连接管道并完成一次请求/响应交换。
    pub fn pipe_client_call(
        endpoint: &str,
        request_line: &str,
        timeout: Duration,
        max_bytes: usize,
    ) -> PlatformResult<String> {
        let name = wide(&format!("\\\\.\\pipe\\{endpoint}"));
        let deadline = Instant::now() + timeout;
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
                        if Instant::now() >= deadline {
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
        unsafe { WriteFile(handle, Some(&payload), Some(&mut written), None) }
            .map_err(|_| last_err("WriteFile"))?;

        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if Instant::now() >= deadline {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(PlatformError::Win32(
                    "Timed out waiting for IPC response".into(),
                ));
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
}

#[cfg(windows)]
pub use win::{PipeServer, pipe_client_call, read_handle_line, write_handle_line};
