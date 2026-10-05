# NeurolingsCE 重写架构规划（Rust + Flutter + winit）

目标：用 Rust + Flutter + winit 完整复刻桌面看板娘应用，**用户可感知的一切**（文案、UI 布局/配色/控件、动画、图标、交互流程、托盘、CLI、HTTP API、包导入/下载/安装、设置持久化）与参考版本 v0.5.3 一致。

---

## 一、总体架构：Rust 单主机进程 + Flutter 引擎嵌入 + winit 窗口层

### 1.1 为什么这样切分

| 关注点 | 归属 | 理由 |
|---|---|---|
| 管理器 UI（七页 + 对话框 + 主题） | Flutter（Dart） | 复杂布局/响应式/滚动/焦点，Flutter 表达力强、可写 golden 测试 |
| 桌宠窗口（透明、点击穿透、多屏、置底） | winit + wgpu（Rust） | 需要分层窗口、逐帧精灵动画、低延迟拖拽，Flutter 不擅长 |
| 窗口创建与事件循环 | winit（Rust） | 统一托管两类窗口，避免两套消息泵打架 |
| 业务逻辑/引擎/网络/IO | Rust crate | 单一事实来源，Flutter 与 CLI/HTTP 共用 |

关键点：**winit 是唯一的窗口与事件源**。管理器窗口不是 Flutter 自带的 Win32 窗口，而是 winit 创建的窗口，通过 **Flutter Embedder API**（`flutter_embedder.h`）把 Flutter 引擎挂上去，用 `glutin`/`wgpu` 提供 GL/Vulkan 合成表面，并配置 `custom_task_runners` 让 Flutter 与 winit 共用同一个事件循环。这样：

- 一个进程、一个事件循环、一个 GPU 栈；
- 桌宠窗口与管理器窗口在同一帧预算内调度；
- 托盘/全局快捷键/多显示器都由 Rust 侧统一处理。

Rust ↔ Dart 通信用 **flutter_rust_bridge**（FFI，自动生成绑定），不走 JSON 管道，避免高频 tick 的序列化开销。

### 1.2 前端技术栈

- 状态：`Riverpod`（可测试、可 golden）
- 路由：`go_router`（七页 + 深链，便于截图自动化）
- 主题：自建 `fluent_kit` 组件包（Fluent/Ela 视觉：圆角、亚克力表面、强调色、focus ring、hover/pressed 状态、暗色/高对比）
- 本地化：Flutter ARB（`app_zh_CN.arb` / `app_en.arb`），字符串以参考版本 TS 文案为准，逐条对齐
- Golden 测试：`flutter test --update-goldens` + `golden_toolkit`

### 1.3 后端 crate 划分（Cargo workspace）

```
NeurolingsCE/
├── crates/
│   ├── player/    桌宠行为引擎：actions/behaviors XML 解析、行为选择、
│   │              动作状态机、条件表达式求值、tick 推进、繁殖/自毁
│   ├── doll/      桌宠窗口：winit 透明窗口、wgpu 精灵渲染、拖拽/鼠标交互、
│   │              右键菜单、气泡、投掷物理
│   ├── stage/     环境模型：屏幕/工作区/活动窗口/IE 窗口边缘、坐标系、
│   │              地板-墙壁判定、多显示器
│   ├── parcel/    资源包：.mascot 与 legacy zip 的读取、校验、转换、导入/导出、
│   │              安全路径（防 zip-slip）、图片检查、缩略图
│   ├── vault/     持久化：设置、组合、会话、模板库索引、迁移
│   ├── atlas/     商店：索引拉取/缓存、下载、SHA-256 校验、安装/取消、状态机
│   ├── postbox/   更新：静态清单解析、资产匹配、校验、安装流程
│   ├── gate/      CLI：参数解析、命令执行、JSON/文本输出
│   ├── bridge/    HTTP 服务 + 本地 IPC（JSONL），命令协议单一定义
│   ├── keeper/    凭据：系统安全存储（DPAPI/keychain/libsecret）
│   ├── fresh/     GitHub Device Flow 登录
│   ├── tool/      音效、日志、进程/单实例、开机自启
│   ├── schema/    共享数据模型与 JSON 契约（被所有 crate 依赖）
│   └── host/      主机：winit 事件循环、Flutter 嵌入、托盘、装配、启动恢复
├── app/           Flutter 应用（Dart）
├── specs/         验收清单、截图对比、字符串清单、各轮记录
└── tools/         对比/采集/打包脚本
```

依赖原则：`schema` 只放数据与契约；业务 crate 之间只依赖 `schema`；UI（Dart）只通过 `flutter_rust_bridge` 调 `host` 暴露的 API，不直接碰引擎。

---

## 二、技术选型（直接用成熟库，不造轮子）

| 需求 | 选型 |
|---|---|
| 窗口/事件 | `winit 0.30` + `raw-window-handle` |
| 渲染 | `wgpu`（桌宠精灵、Flutter 表面） |
| Flutter 嵌入 | Flutter Embedder API（C FFI 绑定）+ `glutin` |
| 异步 | `tokio` |
| 网络 | `reqwest`（rustls） |
| 序列化 | `serde` / `serde_json` |
| XML | `quick-xml`（Shimeji actions/behaviors） |
| 条件脚本 | `rquickjs` / `boa`（`#{...}` 表达式） |
| 压缩包 | `zip` + `sevenz-rust` + `tar`（legacy Shimeji zip/7z/rar） |
| 图片 | `image`（PNG 解码、尺寸/通道校验、缩放） |
| 哈希 | `sha2` |
| 凭据 | `keyring` |
| 托盘 | `tray-icon` + `muda` |
| 音频 | `rodio` |
| 日志 | `tracing` + `tracing-appender` |
| 开机自启 | `auto-launch` |
| FFI 桥 | `flutter_rust_bridge` |

---

## 三、核心子系统设计

### 3.1 桌宠引擎（`player` + `stage` + `doll`）

- **解析**：`actions.xml` → `Action{ Name, Type, BorderType, Animation[Pose{Image, ImageAnchor, Velocity, Duration}], Hotspot[] }`；`behaviors.xml` → `Behavior{ Name, Frequency, Hidden, Condition, NextBehaviorList }`，含 `<Condition>` 嵌套。
- **tick 循环**：40 ms 固定步长（与参考一致），顺序为「刷新环境 → 反向遍历会话 → 推进动作 → 处理繁殖/自毁 → 刷新窗口与气泡」。
- **行为选择**：条件求值为真时按 `Frequency` 加权随机，`Hidden` 行为仅由 `NextBehaviorList` 进入；`Add=false` 表示替换而非追加候选。
- **动作类型**：Stay / Move / MoveWithTurn / Animate / Sequence / Select / Look / Offset / Jump / Fall / Dragged / Thrown / Interact / Breed / Transform / SelfDestruct / Reference / ScanMove / Resist / Instant。
- **环境**：多显示器 → 工作区矩形 → 活动窗口矩形（Windows：`GetForegroundWindow`+`GetWindowRect`；Linux：GNOME/KDE 扩展；macOS：AXUI）；地板/墙壁/窗口边框判定决定 BorderType 行为。
- **窗口**：每只桌宠一个无边框分层窗口，透明背景 + 点击穿透（拖拽/热点区时临时关闭穿透），支持 DPI 缩放与自定义缩放倍率。
- **交互**：左键拖拽（含投掷惯性）、右键菜单、双击热点触发 `Pat` 类行为、气泡展示。

### 3.2 资源包（`parcel`）

- `.mascot` = ZIP 容器，内含 `info.json{name,version,description,author}` + `actions.xml` + `behaviors.xml` + `img/*.png`。
- 导入流程：拖放/选择 → 解包到临时目录 → **安全路径校验**（拒绝 `..`、绝对路径、符号链接逃逸）→ 校验 `info.json` 与 XML → 图片存在性与可读性 → 重命名冲突处理 → 写入模板库 → 刷新列表。
- legacy Shimeji zip → `.mascot` 转换（「制作」页）：分析压缩包内多只桌宠 → 用户勾选 → 生成 `info.json`（可编辑）→ 校验 → 输出到指定目录（不自动导入）。

### 3.3 商店（`atlas`）+ 更新（`postbox`）

- 索引：`https://blog.qingchenyou.asia/NeurolingsCE-Mascots-Staging/index-v1.json`，字段 `id/name/version/summary/authors/license/download{url,size,sha256}/status`。
- 下载 → **先校验 SHA-256** → 再安装到本地模板库；支持取消、进度、失败重试、缓存清理。
- 更新清单：`https://blog.qingchenyou.asia/NeurolingsCE/update/latest.json`，按平台/架构匹配资产，校验后安装。
- GitHub 登录：Device Flow（显示 URL + 验证码，浏览器授权，轮询成功后自动关闭弹窗并刷新账号状态），令牌仅写入系统凭据存储。

### 3.4 边界服务

- **CLI**（`gate`）：与参考版本完全相同的命令、参数、退出码、文本/JSON 输出格式。
- **HTTP API**（`bridge`）：`localhost:32456` REST 路由与响应字段一致。
- **IPC**：本地 JSONL 通道，供 CLI 控制常驻 GUI 运行时。
- **持久化**（`vault`）：沿用 `%LOCALAPPDATA%\NeurolingsCE\`（Windows）/`AppLocalDataLocation`（Linux/macOS）下的 `settings.json`、`combinations.json`、`mascots/`、`mascot-cache/`、`store-index-cache/`，保证老用户数据可直接继承。
- **启动恢复**：开机自启 + 静默启动驻留托盘 + 恢复「上次关闭前组合」或指定组合。
- **日志**：每会话独立文件（`log/YYYY-MM-DD/neurolingsce-HH-mm-ss-zzz.log`），级别 debug/info/warning/error/critical，环境变量 `NEUROLINGSCE_LOG_LEVEL` / `NEUROLINGSCE_LOG_STDERR`。

### 3.5 管理器 UI（Flutter 七页）

| 页面 | 要点 |
|---|---|
| 主页 | 模板列表/详情、召唤、随机召唤、导入、刷新、打开目录、响应式布局、底部状态栏（当前桌宠数 / 模板数） |
| 制作 | legacy zip 检查 → 候选勾选 → `info.json` 编辑 → 转换输出 |
| 组合 | 保存当前桌宠组合、上次关闭组合、恢复/删除 |
| 商店 | 目录浏览/筛选、卡片（名称/版本/简介/来源/大小/许可证/状态）、下载/安装/取消、登录入口、提交入口 |
| Codex | 连接/恢复会话、模式选择、turn 控制、Plan/回复只读区、审批与用户输入卡片、pending 徽标 |
| 设置 | 乘数、气泡、点击、窗口推动、Codex 开关与路径、detach/缩放、背景色（HEX/RGB 取色器）、语言、启动、HTTP、更新/代理 |
| 关于 | 身份区、版本/更新/项目支持可展开卡片、许可证、issue 入口 |

每个页面都必须覆盖：浅色/深色主题、窄宽度流式换行、键盘焦点环、可访问性标签、中文/英文文案。

---

## 四、验收方法论：截图对比 + 流程比对

这是本项目的核心工作，不是一个收尾步骤。

### 4.1 基线采集（参考版本）

1. 用 Qt 6.11.1（`D:\Installation\Qt`）构建参考版本；
2. 用脚本驱动：HTTP API / CLI 设置状态，UI 自动化（Windows UI Automation，按控件名点击导航）走到每个页面与状态；
3. 逐窗口 `PrintWindow` 抓图（含非客户区），按 `页面_状态_主题_语言_DPI.png` 命名存入 `specs/golden/reference/`。

### 4.2 对比

- 新版本同样脚本驱动（深链 + API），抓图到 `specs/golden/candidate/`；
- `tools/compare_shots.py`：对齐尺寸 → 感知差异（允许抗锯齿/字体微调阈值）→ 输出并排图与差异热力图 + HTML 报告；
- 判定：结构差异（控件缺失/错位/颜色偏离）必须零容忍；亚像素级抗锯齿差异记录在案不阻断。

### 4.3 对比矩阵（每一格都要过）

维度：7 个页面 × {默认, 空状态, 加载中, 错误态, 选中态} × {浅色, 深色} × {zh_CN, en} × {100%, 125%, 150% DPI} × 对话框（取色器、进度、详情、提交、登录、许可、检查器）× 桌宠窗口（各行为帧、气泡、拖拽、右键菜单、沙盒窗口模式）× CLI/HTTP 输出 diff。

### 4.4 自动化回归

- Dart：golden 测试 + 组件测试；
- Rust：`cargo test`（引擎、包校验、安全路径、协议字段、商店/更新解析）+ CLI/HTTP 输出快照对比；
- 每次提交后跑 `flutter analyze` + `cargo clippy` + `cargo fmt --check`。

---

## 五、执行阶段（批准后按此顺序推进，每阶段内小任务完成即提交并推送）

| 阶段 | 内容 | 完成判据 |
|---|---|---|
| **0 风险验证** | Flutter Embedder + winit 跑通（winit 窗口内渲染 Flutter 页面）；winit 透明可穿透桌宠窗口跑通 | 两个 demo 二进制可运行并截图 |
| **1 骨架** | workspace、schema、日志、托盘、单实例、Flutter 应用壳 + 路由 + 主题 + 双语 | 应用启动、托盘菜单可用、空七页可导航 |
| **2 引擎** | XML 解析、行为/动作状态机、表达式、环境模型、tick、渲染窗口、拖拽/菜单/气泡 | 内置 Default 桌宠行为与参考一致（逐帧对比） |
| **3 资源包** | .mascot 读写、导入/导出/校验、安全路径、legacy zip 转换、模板库 | 六个官方包 + legacy zip 导入结果一致 |
| **4 管理器七页** | 主页/制作/组合/商店/Codex/设置/关于 全量复刻 | 七页截图对比通过（双主题/双语/多 DPI） |
| **5 边界服务** | CLI、HTTP/JSONL IPC、商店下载安装、更新、GitHub 登录、提交、音效、开机自启、启动恢复 | CLI/HTTP 输出 diff 一致；下载安装流程实测通过 |
| **6 精修与全量验收** | 动画曲线、焦点环、窄屏换行、可访问性、异常态文案、性能（帧预算/内存） | 对比矩阵全绿；性能不低于参考 |
| **7 打包** | Windows 便携包/MSI、Linux 产物、macOS 产物、版本与更新清单 | 产物可安装、可运行、可升级 |

---

## 六、工程纪律

1. **不翻译、不提及参考实现**：所有逻辑按本规划从需求重新实现；注释与文档只描述本仓库自身行为，不出现任何指代参考项目的字样。
2. **不照搬参考的代码组织**：按本规划的 crate/Dart 结构落地，模块边界以职责而非参考文件划分。
3. **优先用现成库**：解析、压缩、网络、渲染、凭据、托盘一律用成熟 crate，不手写。
4. **提交与推送**：每个小任务完成即 `commit` + `push`；提交信息用 Conventional Commits 中文描述；`NeurolingsCE-Qt/` 目录加入 `.gitignore`，永不入库、永不修改。
5. **不留不可用状态**：任何改动必须编译通过、`analyze`/`clippy` 干净、相关测试通过；修不好就回退到上一个可用提交。
6. **禁止无据断言**：每次声称「一致」必须附带本次实际运行的截图对比或输出 diff 证据。

---

## 七、已知风险与应对

| 风险 | 应对 |
|---|---|
| Flutter Embedder + winit 自定义表面是最大技术不确定性 | 阶段 0 先行验证；若 GPU 合成不稳定，退路是 Flutter 软件渲染器（`--enable-software-rendering`）进 wgpu 纹理 |
| 参考版本无预编译产物（仓库 latest release 404）、需本地构建 | 已确认 Qt 6.11.1 可用；如构建受阻，改用仓库内截图 + 源码文案作为基线，并在验收记录中标注来源 |
| 桌宠行为的逐帧一致性（含随机数与条件求值） | 固定 tick 步长 + 可注入种子的 RNG，写帧序列快照测试 |
| 字体与文本度量差异导致像素级不一致 | 统一使用系统 Segoe UI / 中文回退字体，文本区域允许亚像素阈值，布局尺寸零容忍 |
| 工作量巨大，单轮不可能完成 | 按阶段推进，每阶段独立可验收；截图对比贯穿全程而非最后统一做 |

---

**以上为架构规划，现停下等待批准。** 批准后我将从阶段 0 开始执行，期间不再中断或提问。
