# Neurolings-rs 1.0.0 重写与收敛计划

> 行为基准：NeurolingsCE v0.5.3
> 目标版本：Neurolings-rs 1.0.0
> 当前阶段：三平台工程与发布门禁已接入，等待原生 CI 和真机交互验收。

## 1. 架构边界

Neurolings-rs 由三个用户可见进程和七个 Rust crate 组成：

- `NeurolingsCE`：Rust 运行时，负责桌宠状态机、平台窗口、托盘、气泡、音效、HTTP/IPC、更新与 Codex app-server。
- `NeurolingsCE-cli`：Rust CLI，保持 CE v0.5.3 的命令、JSON、退出码和自动启动语义。
- `neurolings_manager`：Flutter Manager，负责 Home、Create、Store、Combinations、Codex、Settings、About 和检查器。
- `neurolings-engine`：XML 解析、行为/动作、物理环境、QuickJS 和广播。
- `neurolings-pack`：`.mascot`、zip/RAR/7z/TAR 导入、SafePath 和资源预算。
- `neurolings-platform`：Windows、Linux X11、macOS 的窗口、活动窗口、IPC、自启和凭据抽象。
- `neurolings-runtime`：进程生命周期、命令调度、operation、模板、会话和外部集成。
- `neurolings-common`、`neurolings-store`：共享契约、商店、登录、投稿和更新清单。

控制面分为公开 HTTP API、带高熵 Bearer token 的 Manager 私有 HTTP API，以及 CLI 使用的本地 IPC。写命令在服务端获得稳定 `operation_id` 后才允许返回 202，Manager 只轮询状态，不重放请求。

## 2. 已完成阶段

| 阶段 | 范围 | 当前结论 |
|---|---|---|
| M0-M2 | workspace、契约、引擎、包格式、CLI 基础 | 已完成 |
| M3-M4 | Windows 窗口、运行时、IPC、HTTP、CLI 运行时命令 | 已完成并经过本轮加固 |
| M5 | Flutter Manager 七页及本地控制面 | 已完成，Windows/Linux/macOS runner 已接入 |
| M6 | Linux X11 与 macOS Rust 平台层 | 代码与构建工程已实现，仍需真机验收 |
| M7 | Store、缓存、Device Flow、投稿客户端 | 客户端完成；服务端部署由维护者负责 |
| M8 | Codex、气泡、组合、自启、窗口模式 | 已完成并经过并发/资源边界加固 |
| M9 | 更新、三平台打包、文档 | 联合发布工作流已接入，等待原生 CI 验证 |

## 3. 本轮修复重点

本轮不再以“模块存在”为完成标准，而以异常输入和状态转换为验收依据：

1. QuickJS：修复负坐标生成非法 JS、每只桌宠默认隔离上下文，并给所有包脚本入口设置 100ms 中断预算。
2. Codex：修复两条 mutex 自锁、中文长消息切片 panic、plan delta 无界增长和断开清理。
3. HTTP/IPC：修复 URL UTF-8 边界 panic、连接计数泄漏、短写、半包及分阶段超时。
4. operation：稳定 id、结果保留、202/504 边界、业务 `state` 保留和 Manager 只轮询协议。
5. 包导入：统一 16 MiB 单文件、100 MiB 总量和 4096 文件预算，把 TAR/7z/RAR 检查前移到分配或写盘之前。
6. 平台：补齐 Windows 行为差异、macOS Manager 生命周期和心跳坐标；记录 Linux RandR 与 Unicode 菜单的依赖阻塞。

详细根因、修复方法和证据见仓库根目录 `CONTRAST_ANALYSIS.md`。

## 4. 发布门禁

2026-08-26 的最终工作树已通过 Rust 194/194、Flutter 5/5、Clippy、全目标检查、格式检查、七模板 2100 tick 和真实 IPC 生命周期验证。以下命令继续作为每个候选版本的强制门禁。

每次准备 1.0.0 候选版本时必须执行：

```powershell
cargo test --workspace --locked
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Flutter Manager 必须执行：

```shell
flutter analyze --no-pub
flutter test --no-pub
flutter build windows --release  # Windows
flutter build linux --release    # Linux
flutter build macos --release    # macOS
```

行为验证还包括七个模板各运行 300 tick、真实 IPC 连续召唤两只 Default、CLI 召唤/关闭/停止、公开 API 路由与私有 token 隔离，以及 `git diff --check`。测试结果必须记录精确通过数量；未在当前工作树运行的命令不能写成通过。

## 5. 未完成与验收边界

以下事项仍阻止“三平台全部完成”的结论：

- Linux 多显示器需要启用 `x11rb` RandR/Xinerama 能力，涉及 `Cargo.toml` 和锁文件。
- Linux 中文菜单需要 Xft/Pango/Cairo 或等价客户端文本渲染，现有 X Core Text 无法可靠显示 Unicode。
- macOS Manager 已通过三秒窗口可见性心跳弥合应用级 hidden 状态差异，仍需在真机验证 `orderOut`、激活与多屏组合。
- Windows 混合 DPI、Linux X11/XWayland、macOS 多屏和辅助功能权限必须分别执行真机验收。

这些任务不通过删除测试、放宽断言或声明“构建通过等同真机验收”来关闭。后续平台依赖、runner 或打包脚本变更必须继续经过对应宿主的原生 CI。

## 6. 完成定义

1. Windows 全量门禁、模板压力和真实 IPC 验证全部通过。
2. Linux/macOS runner 可构建，Rust 与 Flutter 均在目标系统启动成功。
3. 三平台完成桌宠透明渲染、点击命中、菜单、托盘、Manager 显隐、多屏召唤和活动窗口交互清单。
4. `CONTRAST_ANALYSIS.md` 中所有 P0/P1 项要么有测试证明已修复，要么明确标注外部阻塞与责任人。
5. 最终产物版本统一为 1.0.0，CLI、HTTP `app_info` 和 Manager About 显示一致。
