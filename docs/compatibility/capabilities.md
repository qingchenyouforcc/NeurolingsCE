# NeurolingsCE 兼容性基线

本文记录 Rust 重写前从当前工作树核对出的可观察能力。资料来源只包括
`E:/Projects/NeurolingsCE-Qt` 下的源码、头文件、README、API 文档和资源文件；
本基线不读取 Git 历史，也不以历史提交推断行为。实现阶段应把这里的条目转成
Rust 的 golden fixture、协议测试和跨平台手工验收记录。

## 应用、版本和入口

- 版本单一来源是 `VERSION.txt`：当前版本为 `0.5.3`，应用名为 `NeurolingsCE`，
  bundle id 为 `io.github.qingchenyouforcc.NeurolingsCE`。
- GUI 入口为 `src/app/main.cc`，独立 CLI 入口为 `src/app/cli_main.cc`；总体数据流
  和线程边界见 `src/app/README.md`。
- runtime 由 `src/app/runtime/` 负责模板、会话、屏幕环境和生命周期；GUI tick
  周期为 40 ms，每个 tick 拆成 4 个 subtick（
  `src/app/runtime/ManagerRuntimeHelpers.hpp`、`src/app/ui/ManagerWindowSetup.cc`）。

## CLI 基线

- 解析和执行入口为 `src/app/cli/CommandLineParser.cc` 与
  `src/app/cli/CommandExecutor.cc`，输出由 `src/app/cli/OutputFormatter.cc` 统一。
- 文档命令包括 `--help/-h`、`--summon/-s`（按名称、data id 或 random）、
  `--close`、`--close-all`、`--stop`、`--mascot/-m list|add|remove|validate`、
  `--list/-l`、`--version/-v` 和 `--codex-notify`。
- 全局选项包括 `--quiet`、`--json`、`--connect-timeout-ms`、
  `--read-timeout-ms`；JSON 模式输出单行 JSON。兼容命令为
  `list`、`list-loaded`、`spawn`、`alter`、`dismiss`、`dismiss-all`。
- runtime 命令通过本地 IPC，可在 runtime 未运行时自动启动；模板管理可以独立运行；
  Codex notify 不会为了回调自动启动 runtime。`--host` 与 `--port` 已明确不支持。
- CLI 标签是当前进程内的用户标签，与 runtime mascot id 分离，不持久化。

## 本地 IPC 基线

- 实现和客户端位于 `src/app/core/localipc/ShijimaLocalApi.cc`、
  `src/app/core/localipc/ShijimaLocalApiClient.cc`，协议为一行一个紧凑 JSON object。
- server name 固定为 `io.github.qingchenyouforcc.NeurolingsCE.cli`，定义在
  `ShijimaLocalApiClient.cc`。
- 请求/响应消息上限为 1 MiB（`include/shijima-qt/SecurityLimits.hpp`），客户端默认
  连接和读取超时均为 500 ms；服务端读取/写回超时为 2 s。
- 监听失败时先 ping 旧 endpoint，确认 stale 后才移除；停止通过唤醒连接退出 worker。
  非法 JSON、未完成行、超限消息、连接关闭和业务错误必须保持可区分。
- 业务 JSON 由 `src/app/core/commands/MascotApi.cc` 与
  `MascotCommandDispatcher.hpp` 统一，IPC worker 不直接访问 QWidget。

## HTTP 基线

- HTTP 服务实现为 `src/app/core/http/ShijimaHttpApi.cc`，文档为
  `src/docs/HTTP-API.md`；默认关闭，仅显式启用时启动，并绑定 `127.0.0.1`。
- 基础地址为 `http://127.0.0.1:32456/shijima/api/v1`；JSON body 上限为 1 MiB。
- 路由包括：
  `GET/POST /mascots`、`GET/PUT/DELETE /mascots/:id`、
  `DELETE /mascots`、`GET /loadedMascots`、
  `GET /loadedMascots/:id`、`GET /loadedMascots/:id/preview.png`、
  `GET /ping`、`POST /cli/labels`、`GET /cli/labels/:label`。
- 传输层只负责 body、Content-Type、JSON object、状态码和响应 framing 校验；
  业务仍经 `MascotCommandService` 在 GUI 线程执行。未知方法/路径返回 bad request。

## 资产与安全基线

- 资产导航和调用链见 `src/app/core/assets/README.md`；核心实现为
  `src/app/core/assets/MascotPackage.cc`、`SafePath.cc`、`AssetLoader.cc`、
  `MascotData.cc`。
- 导入流程支持 `.mascot`、ZIP/legacy archive 和模板目录，必须先检查包名、manifest、
  `actions.xml`、`behaviors.xml`、PNG 与音频，再原子安装到受控目录。
- `SafePath::safeChildPath` 拒绝绝对路径、盘符、空路径段、`.`/`..`、分隔符逃逸和
  canonical path 越界；内置 `@` 根是特殊只读资源根。
- 当前限制来自 `include/shijima-qt/SecurityLimits.hpp`：压缩包 100 MiB、解压总量
  100 MiB、单文件/音频 16 MiB、单图 4096×4096 像素、全部图片 256 MiB 像素预算、
  archive entry 最多 4096，IPC/HTTP JSON 各 1 MiB。
- Rust 重写必须对路径穿越、符号链接逃逸、超限 archive、巨型图像和损坏 manifest
  保持 fail-closed；失败不能留下半安装包。

## 引擎与 runtime 基线

- 引擎导航见 `src/app/core/shijima-engine/README.md` 及其
  `shijima/{action,behavior,broadcast,mascot,scripting}` 子目录。
- 数据流为 `MascotData → parser → factory → behavior manager → action/state/
  environment → runtime → UI`；引擎不创建 Qt 窗口。
- actions/behaviors XML、动作边界、行为选择、广播、脚本上下文、hotspot、
  位置/屏幕环境和繁殖/自毁都属于兼容面；脚本异常、超时或资源超限要拒绝执行。
- runtime 导航见 `src/app/runtime/README.md`。Manager、引擎 manager、QSettings 和
  widget 归 GUI 线程；IPC/HTTP/import worker 只能通过同步 dispatch 进入 GUI。
- mascot session 在当前 tick 反向遍历，删除采用待删除标记并在 tick 后提交；
  关闭顺序为停止新请求和 timer → 停止 tray/API → 删除会话/窗口 → 注销模板和引擎资源。

## Codex 基线

- 配置与 app-server 边界见 `src/app/core/codex/README.md`；
  实现为 `CodexAppServerProtocol.cc`、`CodexAppServerClient.cc`。
- app-server 仅由用户显式连接启动，使用 `codex app-server --listen stdio://`；
  stdout 为有界 UTF-8 JSONL，stderr 只保留限长诊断，不观察其他会话。
- 单行上限 4 MiB，未完成 stdout buffer 上限 8 MiB，pending approval 最多 16 条；
  超限、非法 envelope、断线和未知 approval 必须 fail-closed。
- JSON-RPC request id 保留字符串或安全整数；未知 server request 返回 `-32601`；
  approval、`requestUserInput`、Plan/reply 状态只保存在内存。
- 关闭顺序为 best-effort cancel → 清空 pending 状态 → terminate/kill；不自动批准、
  自动重启或把用户消息、命令、cwd、diff、理由和 Plan 正文写入日志/磁盘。
- 相关回归测试基线见 `src/app/tests/CodexAppServerTests.cc` 与
  `src/app/tests/README.md`。

## 平台与发布基线

- 平台代码位于 `src/platform/Platform/{Windows,Linux,macOS,Stub}`，能力包括屏幕/
  工作区、光标、active window、窗口推动、透明/置顶/穿透窗口和系统安全凭据。
  不支持的能力必须显式降级，不能伪造成功。
- GitHub 登录与凭据边界见 `src/app/core/github/README.md`：
  Windows Credential Manager、macOS Keychain、Linux Secret Service（未启用时明确不可用），
  token 不得明文落盘或写日志。
- 商店、更新和投稿入口分别见 `src/app/core/mascotstore/README.md`、
  `src/app/core/update/README.md`、`src/app/core/submission/README.md`：
  HTTPS/回环下载限制、ETag/Last-Modified、`.part` + SHA-256、取消、串行安装、
  Device Flow、multipart 投稿和结构化错误属于兼容面。
- 当前发布/打包脚本和资源位于 `src/tools/`、`installer/`、`packaging/`、
  `src/resources/`、`translations/`。目标产物基线为 Windows x64（ZIP/MSI/setup）、
  Linux x86_64/arm64（AppImage）和 macOS x86_64/arm64（`.app` ZIP），并生成
  SHA-256 校验清单。

## 复核规则

实现每个 Rust crate 时，应为本节对应条目补充 fixture 或测试，并以当前工作树文件为
行为证据。任何无法核对的格式、平台能力或协议字段必须记录为阻塞项，不得用默认值或
静默降级宣称“兼容”。
