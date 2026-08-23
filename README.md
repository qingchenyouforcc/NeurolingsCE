# Neurolings-rs

NeurolingsCE 的 Rust + Flutter 重写：跨平台桌面看板娘（Shimeji）运行器。
原 C++/Qt 实现见 [NeurolingsCE](https://github.com/qingchenyouforcc/NeurolingsCE-QT)；
本仓库按 `docs/REWRITE_PLAN.md` 的里程碑完成迁移。

## 架构

```
manager/ (Flutter, fluent_ui)          ← 管理器 UI（七页导航）
   │ HTTP 127.0.0.1:32456 + Local IPC
crates/
 ├─ neurolings-engine    Shimeji-ee 行为引擎（QuickJS 条件脚本，22 种 action）
 ├─ neurolings-pack      .mascot 包格式 / legacy zip 导入 / 路径安全 / 校验
 ├─ neurolings-platform  平台层：透明置顶窗口(Win/X11/AppKit)、IPC、autostart、气泡
 ├─ neurolings-runtime   运行时守护进程（NeurolingsCE.exe）：tick、渲染、HTTP/IPC 服务
 ├─ neurolings-cli       NeurolingsCE-cli.exe：独立模板管理 + 运行时控制
 ├─ neurolings-store     商店索引/下载、GitHub Device Flow、投稿、更新器
 └─ neurolings-common    共享契约与常量
```

- 桌宠窗口由 Rust 原生渲染：Windows `UpdateLayeredWindow` 逐像素 alpha +
  命中穿透；Linux X11 ARGB + XFixes input-shape；macOS NSView `hitTest`。
- CLI / HTTP API / IPC 端点名与输出契约保持与 C++ 版本兼容
  （见 `docs/HTTP-API.md` 与 `docs/contracts/cli-contract.md`）。

## 构建

```powershell
# Rust（运行时 + CLI + 全部 crate）
cargo build --release
cargo test --workspace

# Flutter 管理器
cd manager && flutter pub get && flutter build windows --release
```

## 运行

```powershell
# 桌宠运行时（托盘 + 透明桌宠窗口）
.\target\release\NeurolingsCE.exe

# CLI 控制
.\target\release\NeurolingsCE-cli.exe --json --mascot list
.\target\release\NeurolingsCE-cli.exe --json --summon mascot --name @ 1
.\target\release\NeurolingsCE-cli.exe --json --list
.\target\release\NeurolingsCE-cli.exe --json --stop

# 无窗口自检（CI 用）
.\target\release\NeurolingsCE.exe --smoke 300

# HTTP API（公开端口由设置页「更新」/HTTP 相关，或设置 update/checkOnStartup 同组的
# http/enabled 控制默认关闭；Manager 内部经私有管理端口 32457 通信，开箱即用）
curl http://127.0.0.1:32456/shijima/api/v1/ping
```

## 打包

见 `packaging/README.md`：`packaging/package-windows.ps1` 汇总 release 目录
并生成 `SHA256SUMS.txt`；更新清单遵循 NeurolingsCE 的 `updater-schema`。

## 状态

各里程碑完成情况见 `docs/REWRITE_PLAN.md` 的进度记录。Linux/macOS 的窗口
后端已通过交叉编译检查，需真机做视觉验证；商店/投稿服务端与 GitHub App 配置
需维护者部署后方可启用（与原项目计划一致）。
