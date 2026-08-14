# NeurolingsCE → Rust + Flutter 重写规划

> 源项目：`E:/Projects/NeurolingsCE`（C++17 / Qt 6.8，v0.5.3）
> 新项目：`Neurolings-rs`
> 决策基线：Rust 渲染桌宠 + Flutter 管理器；v1 全量对齐 0.5.3；三平台并行；包格式与 CLI/HTTP API 完全兼容。

## 1. 总体架构

```
┌─────────────────────────────────────────────────────────┐
│ Flutter 管理器 (neurolings_manager)                      │
│  Home / Create / Store / Combinations / Codex / Settings │
└──────────────┬──────────────────────────────────────────┘
               │ HTTP 127.0.0.1:32456 (读) + Local IPC (控制)
┌──────────────▼──────────────────────────────────────────┐
│ Rust 运行时守护进程 (neurolingsd / NeurolingsCE.exe)      │
│  ├─ 引擎: Shimeji-ee XML 解析 → 行为/动作状态机 → 物理     │
│  ├─ 渲染: 每只桌宠一个透明无边框置顶窗口 (winit+softbuffer)│
│  ├─ 平台层: Win / macOS / Linux(X11+Wayland)              │
│  ├─ Local IPC (命名管道/Unix socket, JSON lines)          │
│  ├─ HTTP API (/shijima/api/v1, 路由契约不变)              │
│  ├─ 商店/登录/投稿客户端, 更新检查, 音频, 托盘             │
└──────────────▲──────────────────────────────────────────┘
               │ Local IPC (与现契约一致)
┌──────────────┴──────────────────────────────────────────┐
│ Rust CLI (NeurolingsCE-cli) — 命令与退出码完全兼容        │
└─────────────────────────────────────────────────────────┘
```

进程模型：运行时与 CLI 为两个 Rust 二进制（对应 `NeurolingsCE` / `NeurolingsCE-cli`）；
Flutter 管理器由运行时拉起或独立启动，通过 HTTP+IPC 通信。单实例、IPC 唤醒管理器
等行为保持。

## 2. 仓库布局

```
Neurolings-rs/
├── Cargo.toml                  # workspace
├── crates/
│   ├── neurolings-engine/      # libshijima 移植: parser/actions/behaviors/physics/broadcast
│   ├── neurolings-pack/        # .mascot 读写、Shimeji zip/rar/7z 导入、路径安全、validate
│   ├── neurolings-platform/    # 三平台抽象: 透明窗口/置顶/活动窗口追踪/自启/凭据存储
│   ├── neurolings-runtime/     # 守护进程: 窗口管理、tick 循环、IPC/HTTP 服务、托盘、音频
│   ├── neurolings-cli/         # CLI 二进制 (NeurolingsCE-cli)
│   └── neurolings-common/      # 共享类型: API/IPC JSON 契约、错误、日志
├── manager/                    # Flutter 应用 (fluent_ui)
├── assets/DefaultMascot/       # 从旧仓库原样迁移
├── mascot_pack/                # 测试用吉祥物包 (golden fixtures)
├── installer/                  # WiX MSI (Windows) / dmg / AppImage
├── docs/                       # HTTP-API.md、迁移指南、架构文档
└── .github/workflows/          # 三平台 CI + 契约测试
```

## 3. 技术选型

| 子系统 | Rust 选型 | 说明 |
|---|---|---|
| 窗口/渲染 | `winit` + `softbuffer` + `tiny-skia` | CPU 渲染 PNG 帧，25FPS；逐像素命中测试 |
| 异步/服务 | `tokio` + `axum` | HTTP API；IPC 用 `interprocess`（命名管道+UDS 统一） |
| XML | `roxmltree` / `quick-xml` | 移植 rapidxml 解析逻辑 |
| 压缩包 | `zip` + `unrar` + `sevenz-rust` | 替代 libarchive/unarr；导入分析逻辑移植 libshimejifinder |
| 图像 | `image` + `png` | 帧解码、mask 生成 |
| HTTP 客户端 | `reqwest` (rustls) | 商店/登录/投稿/更新 |
| 凭据 | `keyring` | 替代三平台 CredentialStore |
| 音频 | `rodio` | wav 音效（可选 feature） |
| 托盘 | `tray-icon` | |
| Windows API | `windows-rs` | UpdateLayeredWindow、WinEventHook、注册表自启 |
| macOS | `objc2` + `core-graphics` | NSWindow/Accessibility |
| Linux | `x11rb`（Shape 扩展）+ `gtk-layer-shell`（Wayland） | Wayland 为最大平台风险 |

Flutter 侧：`fluent_ui`、`window_manager`、`http`、`flutter_riverpod`、arb 国际化（en/zh_CN）。

## 4. 子系统重写要点

- **引擎**（最核心）：按 `src/app/core/shijima-engine` 逐模块移植 —— parser → factory →
  22 种 action（walk/fall/jump/breed/dragged/interact/transform/sequence/select/scanmove…）
  → behavior manager（频率/条件）→ environment（gravity、floor/ceiling/work area、多屏）。
  tick 模型保持 40ms/subtick。**验收用 golden-state 测试**：同一 mascot_pack 输入，
  逐 tick 对比新旧引擎的 state 序列（位置/动作/帧）。mascot 包未使用 JS 脚本，
  scripting（duktape）后置，不进入 v1 关键路径。
- **包格式**：`.mascot`（info.json/actions.xml/behaviors.xml/img/sound/bubble_context.txt）
  读写 + 旧 Shimeji-ee zip 导入 + SafePath 安全检查 + `--mascot validate`
  退出码 0/1/2 契约。
- **渲染窗口**：每桌宠一个透明无边框置顶工具窗口；Windows 用 `UpdateLayeredWindow`
  做逐像素 alpha+命中穿透；macOS 用 nonactivating panel + `ignoresMouseEvents`；
  Linux X11 用 Shape 扩展，Wayland 用 layer-shell（GNOME 支持弱，降级策略：
  不透明区域点击+装饰提示）。
- **CLI/IPC/HTTP**：命令集、JSON 输出、IPC 端点名
  `io.github.qingchenyouforcc.NeurolingsCE.cli`、HTTP 路由全部按
  `docs/HTTP-API.md` 实现，保证现有 agent 技能（neurolingsce-skill/companion）零修改可用。
- **Flutter 管理器**：还原 Home/Create（zip 检查转 .mascot）/Store/Combinations/Codex/
  Settings/About 七页 + 托盘菜单 + 检查器；气泡窗由 Rust 侧渲染（属于桌宠窗口层）。
- **商店/登录/投稿**：按契约重写客户端（索引/缓存/ETag/SHA-256、Device Flow、
  两阶段 HMAC 会话投稿），服务端 Python 工具链直接复用不重写。
- **Codex 集成**：config.toml block 管理 + app-server JSON-RPC 客户端，行为对齐现状
  （显式连接、不自动批准）。
- **其余**：更新检查（updater-schema 校验+sha256）、开机自启/静默恢复、窗口模式、
  多屏、缩放、多语言。

## 5. 里程碑

| # | 内容 | 验收 |
|---|---|---|
| M0 | workspace 脚手架、CI、契约测试基线（从旧仓库导出 CLI/HTTP golden） | `cargo build` 绿、CI 三平台 |
| M1 | 引擎移植 + golden-state 测试 | 6 个 mascot_pack 全部通过逐 tick 对比 |
| M2 | 包格式 + CLI 独立命令（list/add/remove/validate） | 与旧 CLI 输出 diff 为空 |
| M3 | 运行时：窗口渲染、拖拽/右键、物理、托盘（Windows 先行，平台抽象就位） | 手动冒烟清单全过 |
| M4 | IPC + HTTP API + CLI 运行时命令 + 标签 | 契约测试 + neurolingsce-skill 实测 |
| M5 | Flutter 管理器七页 + 主题 + i18n | 页面截图对比 + 金圈路径 |
| M6 | macOS + Linux 平台层补齐 | 三平台 CI 冒烟 |
| M7 | 商店 + GitHub 登录 + 投稿 | 对 staging 服务 E2E |
| M8 | Codex 集成 + 语音气泡 + 组合 + 自启/静默恢复 + 窗口模式 | 功能对齐 checklist |
| M9 | 更新器 + MSI/dmg/AppImage 打包 + 文档 + 迁移指南 | v1.0 发布候选 |

## 进度记录

- M0 完成：workspace 六 crate 骨架、契约资产迁移（DefaultMascot/mascot_pack/HTTP-API.md）、三平台 CI、CLI 输出契约文档（源自 C++ OutputFormatter 精确导出）。
- M1 完成：引擎全量移植（parser+日文翻译器、22 种 action、behavior manager、environment/physics、broadcast、QuickJS 脚本上下文+100ms 中断保护、确定性 Math.random）；43 项测试含确定性逐 tick 重放与 7 包冒烟。跨 C++ 二进制 diff 待 Qt6 构建环境。
- M2 完成：neurolings-pack（info.json/SafePath/命名/inspect/validate/extract/write/install/legacy 分析转换/安全限制/存储路径）+ NeurolingsCE-cli 独立命令（--version/--help/--mascot list|add|remove|validate，退出码 0/1/2 与 JSON 契约对齐）；运行时命令预留 M4。rar/7z 暂返回 Unsupported。
- M3 完成：neurolings-platform Windows 后端（WS_EX_LAYERED 置顶工具窗口、UpdateLayeredWindow 逐像素 alpha+命中穿透、事件队列、弹出菜单、多显示器枚举）+ neurolings-runtime（模板加载/内嵌默认桌宠、10ms tick、帧缓存/镜像/预乘 BGRA、拖拽/右键菜单/hotspot、fall-through 700px、托盘图标、--smoke headless 模式）。GUI 实测 6 秒稳定运行无崩溃；--smoke 300 tick 通过。
- M4 完成：Local IPC（命名管道 JSON lines，端点名兼容 C++；单实例 FILE_FLAG_FIRST_PIPE_INSTANCE 守卫+show_manager 转发）、命令服务层（ping/list/spawn/alter/dismiss/stop/labels/preview，主线程执行+通道应答）、内置 HTTP/1.1 服务（127.0.0.1:32456，/shijima/api/v1 全路由含 preview.png base64、404 契约）、CLI 运行时命令全量接入（--list/--summon/--close/--close-all/--stop 自动启动语义、退出码契约）。E2E 实测：CLI 召唤/标签/关闭/停止、HTTP spawn/单查/预览/404 全部通过；43 项测试绿、clippy 零警告。
- M5 完成：Flutter 管理器（fluent_ui，Windows 构建通过）：导航七页（主页/制作/商店/组合/Codex/设置/关于），主页实时显示运行时状态+已装模板召唤+运行中桌宠关闭，制作页经 CLI 导入 zip/.mascot，设置页语言切换（en/zh arb + gen-l10n），运行时 exe/CLI 自动发现与拉起。widget 测试通过；与运行时+CLI 同目录部署 E2E 联调成功（管理器在线显示 Default 桌宠）。商店/组合/Codex 页为占位，随 M7/M8 落地。
- M6 完成：跨平台 IPC 传输层（Windows 命名管道 + Unix socket 统一抽象）；Linux X11 后端（x11rb 纯 Rust：32 位 ARGB 视觉、_NET_WM_STATE_ABOVE 置顶、XFixes input-shape 逐像素命中穿透、多显示器 _NET_WORKAREA）；macOS 后端（objc2 define_class 自定义 NSView：drawRect 绘帧、hitTest 逐像素命中、NSWindow 无边框浮动层、BGRA→RGBA 预乘转换）。Linux/macOS 后端经 rustup 交叉 target cargo check 全部通过（本机 Windows 无法运行验证，CI 三平台编译兜底）；Wayland 经 XWayland 运行（文档注明）。
- M7 完成：neurolings-store crate——索引模型/解析/语义化版本比较/查询过滤、原子缓存（index+previous+meta 轮换、ETag/Last-Modified）、SHA-256 校验下载、GitHub Device Flow（device_code/轮询/slow_down/凭据存储 keyring+内存双实现）、投稿客户端（两阶段 HMAC 会话 token + multipart 上传 + 幂等键）。6 项单测绿。商店/投稿服务端与 GitHub App 配置仍需维护者部署（与原 C++ 计划一致），Flutter 商店页待服务端就绪接线。
- M8 完成：Codex 集成（~/.codex/config.toml 标记块安装/卸载 + --codex-notify → IPC → 桌宠气泡）、语音气泡（Windows GDI 双 DC 掩码渲染圆角气泡窗口，随 tick 定位/过期回收）、桌宠组合（save/restore/list/delete，combinations.json 原子持久化，restore 清场重建）、开机自启（Windows 注册表 Run 键 + Linux XDG autostart + IPC set/get_autostart）、窗口模式（沙盒画布合成渲染，IPC set_window_mode）、HTTP 通用 POST /command 透传。E2E 实测：召唤→存组合→恢复组合（spawned=2）→删组合→气泡/窗口模式/autostart/codex 全部通过；51+ 项测试绿。
- M9 完成：更新器（neurolings-store::updater——清单 schema 校验、min_supported_version 强制更新判定、平台资产键、SHA-256 下载校验，5 项单测）、Windows 打包脚本（packaging/package-windows.ps1：runtime+CLI+管理器汇总+SHA256SUMS.txt）、项目 README 与打包文档。最终门禁：56 项测试全绿、clippy -D warnings 零警告、cargo fmt 干净、--smoke 150 tick 通过。

## 遗留事项（需真机/维护者介入）

1. Linux/macOS 桌宠窗口需真机视觉验证（本机为 Windows，仅交叉编译检查）。
2. 商店/投稿服务端、GitHub App、Pages 需维护者部署后启用 Flutter 商店页接线。
3. 跨 C++ 二进制逐 tick golden diff 需 Qt6/MSVC 构建环境。
4. MSI 签名与正式发布流水线（WiX）待发布时搭建。

## 6. 主要风险

1. **Wayland 透明置顶窗**（高）：GNOME/Wayland 无真正"屏幕任意位置 overlay"——
   X11 完整支持，Wayland 用 layer-shell 并明确降级文档。
2. **引擎行为逐帧对齐**（中）：golden-state 测试是核心防线，必要时读 C++ 源码修正。
3. **rar 格式支持**（低）：`unrar` crate 依赖 unrar 库，不可行时用
   `compress-tools`(libarchive) 兜底。

## 7. 契约基线（从旧仓库迁移/导出）

- `assets/DefaultMascot/`：内嵌默认桌宠（info.json + XML + 46 PNG）
- `mascot_pack/`：6 套吉祥物包（Default/Neuron/Tuteling/Vedaling/Eviling/Weuron/Cerber）
- `docs/HTTP-API.md`：HTTP API 契约文档
- CLI golden 输出：用旧 build 导出 `--json` 各命令输出作为 diff 基线
- IPC 端点名、设置键、存储路径布局（`AppLocalDataLocation/mascots`、`mascot-cache`）

## 8. 环境

- Rust 1.97.1（cargo 1.97.1）
- Flutter 3.44.8 / Dart 3.12.2
- 开发主机：Windows 11 x64，Git Bash
