//! 本地 IPC 端点（平台传输上的 JSON 行协议）。
//! 端点名与原版运行时保持字节级兼容。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;

use serde_json::Value;

use crate::services::{self, PendingCommand};

/// IPC 同时处理的连接数上限，避免慢连接无限创建线程。
const MAX_CONNECTIONS: usize = 32;

/// 运行时本地 IPC 服务端。
pub struct IpcServer {
    transport: neurolings_platform::ipc::IpcServerTransport,
}

// 传输句柄/套接字只由唯一的 IPC 服务线程访问。
unsafe impl Send for IpcServer {}

impl IpcServer {
    /// 绑定单实例 IPC 端点。
    pub fn bind() -> Result<Self, String> {
        match neurolings_platform::ipc::IpcServerTransport::bind(
            neurolings_common::ipc::IPC_ENDPOINT,
        ) {
            Ok(transport) => Ok(Self { transport }),
            Err(err) => Err(err.to_string()),
        }
    }

    /// 持续接受并有界处理 IPC 连接。
    pub fn serve(mut self, tx: Sender<PendingCommand>) {
        let active = Arc::new(AtomicUsize::new(0));
        loop {
            let Ok(mut connection) = self.transport.accept_client() else {
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            };
            let Some(slot) = ConnectionSlot::acquire(&active) else {
                let response =
                    services::error_json("Too many IPC connections", "too_many_connections", 503);
                let _ =
                    connection.write_line(&serde_json::to_string(&response).unwrap_or_default());
                crate::log::warn("ipc", "connection limit reached, rejecting connection");
                continue;
            };
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _slot = slot;
                handle_connection(connection, tx);
            });
        }
    }
}

struct ConnectionSlot {
    active: Arc<AtomicUsize>,
}

impl ConnectionSlot {
    fn acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::Release);
            return None;
        }
        Some(Self {
            active: Arc::clone(active),
        })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Release);
    }
}

fn handle_connection(
    mut connection: neurolings_platform::ipc::IpcConnection,
    tx: Sender<PendingCommand>,
) {
    match connection.read_line(services::MESSAGE_MAX_BYTES) {
        Ok(Some(line)) => {
            let response = match serde_json::from_str::<Value>(&line) {
                Ok(request) if request.is_object() => services::call(&tx, request),
                _ => services::error_json("Failed to parse IPC request", "bad_request", 400),
            };
            let _ = connection.write_line(&serde_json::to_string(&response).unwrap_or_default());
        }
        Ok(None) => {}
        Err(err) => {
            let response = services::error_json(&err.to_string(), "bad_request", 400);
            let _ = connection.write_line(&serde_json::to_string(&response).unwrap_or_default());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_slots_enforce_limit_and_release_on_drop() {
        let active = Arc::new(AtomicUsize::new(0));
        let slots: Vec<_> = (0..MAX_CONNECTIONS)
            .map(|_| ConnectionSlot::acquire(&active).unwrap())
            .collect();
        assert!(ConnectionSlot::acquire(&active).is_none());
        assert_eq!(active.load(Ordering::Acquire), MAX_CONNECTIONS);

        drop(slots);
        assert_eq!(active.load(Ordering::Acquire), 0);
        assert!(ConnectionSlot::acquire(&active).is_some());
    }
}
