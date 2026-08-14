//! 本地 IPC 端点（平台传输上的 JSON 行协议）。
//! 端点名与原版运行时保持字节级兼容。

use std::sync::mpsc::Sender;

use serde_json::Value;

use crate::services::{self, PendingCommand};

pub struct IpcServer {
    transport: neurolings_platform::ipc::IpcServerTransport,
}

// 传输句柄/套接字只由唯一的 IPC 服务线程访问。
unsafe impl Send for IpcServer {}

impl IpcServer {
    pub fn bind() -> Result<Self, String> {
        match neurolings_platform::ipc::IpcServerTransport::bind(
            neurolings_common::ipc::IPC_ENDPOINT,
        ) {
            Ok(transport) => Ok(Self { transport }),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn serve(self, tx: Sender<PendingCommand>) {
        loop {
            let Ok(mut connection) = self.transport.accept_client() else {
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            };
            match connection.read_line(services::MESSAGE_MAX_BYTES) {
                Ok(Some(line)) => {
                    let response = match serde_json::from_str::<Value>(&line) {
                        Ok(request) if request.is_object() => services::call(&tx, request),
                        _ => {
                            services::error_json("Failed to parse IPC request", "bad_request", 400)
                        }
                    };
                    let _ = connection
                        .write_line(&serde_json::to_string(&response).unwrap_or_default());
                }
                Ok(None) => {}
                Err(err) => {
                    let response = services::error_json(&err.to_string(), "bad_request", 400);
                    let _ = connection
                        .write_line(&serde_json::to_string(&response).unwrap_or_default());
                }
            }
            self.transport.end_connection();
        }
    }
}

/// 客户端一次性调用（CLI 与管理器运行时检测使用）。
pub fn client_call(request: &Value, timeout: std::time::Duration) -> Result<Value, String> {
    let line = serde_json::to_string(request).map_err(|e| e.to_string())?;
    let response = neurolings_platform::ipc::ipc_client_call(
        neurolings_common::ipc::IPC_ENDPOINT,
        &line,
        timeout,
        services::MESSAGE_MAX_BYTES,
    )
    .map_err(|e| e.to_string())?;
    serde_json::from_str(&response).map_err(|e| format!("Failed to parse IPC response: {e}"))
}
