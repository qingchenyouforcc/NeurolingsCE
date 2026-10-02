# NeurolingsCE（Rust 重写）

这是 NeurolingsCE 的独立 Rust 工作区。应用层不依赖 Qt、C++ 或 Duktape；表达式、XML
动作/行为、资源包校验、协议服务和 GUI 都由 Rust crate 组成。

## 运行

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run -p app -- --version
cargo run -p app                 # 本地 TCP IPC：127.0.0.1:32457；HTTP：127.0.0.1:32456
cargo run -p app -- --gui        # egui/eframe 管理器窗口
cargo run -p cli -- --json --list
```

应用默认同时提供回环地址上的 JSONL 控制服务和 HTTP API。CLI 使用
`NEUROLINGSCE_IPC_ADDR` 覆盖 IPC 地址；`--stdio` 可把应用切换为单进程 JSONL 模式，适合
脚本和测试。

## 工作区边界

- `api`：稳定 JSON 字段、错误和 selector/anchor/label 校验。
- `assets`：安全路径、元数据、ZIP/TAR/GZIP/7z/RAR 归档读取及解包限制。
- `engine`：纯 Rust 表达式引擎和 Shijima 风格 XML 动作/行为运行时。
- `runtime`、`services`：会话生命周期、40ms tick 和命令调度。
- `ipc`、`http`、`cli`、`app`：传输、路由、命令行和桌面入口。
- `platform`、`auth`、`store`、`update`、`submission`、`codex`：平台及外围服务边界。
- `ui`：egui/eframe 管理器与 mascot viewport 基础渲染。

内置 `resources/DefaultMascot` 保留行为 XML、元数据和 PNG 帧；导入模板会经过同一套归档
和路径安全限制。跨平台原生窗口、托盘、系统钥匙串和安装器仍通过各平台适配层继续扩展，
不会把平台调用塞进引擎或协议层。
