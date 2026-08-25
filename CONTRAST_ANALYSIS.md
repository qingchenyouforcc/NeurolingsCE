# Neurolings-rs 1.0.0 全量审计与修复报告

> 审计日期：2026-08-26
> 行为基准：NeurolingsCE v0.5.3（C++17 / Qt 6.8）
> 目标实现：Neurolings-rs 1.0.0（Rust + Flutter）
> 审计方式：GPT-5.6 Sol（max）逐模块源码对照、协议检查、边界测试、真实进程验证；修复由多个 GPT-5.6 Luna（max）并行落实，并由主线复核。

## 一、结论摘要

本次审计推翻了此前“核心功能已经接近完全对齐”的宽松判断。重写版本并非只有零散 UI 差异，而是在脚本执行、控制面鉴权、异步命令、IPC 超时、压缩包导入、多显示器坐标和跨平台窗口管理等关键路径上同时存在缺陷。其中若干问题会直接造成桌宠无法运行、运行时永久挂起、管理命令被重复执行、恶意包耗尽内存，或者在特定多屏布局下持续生成非法 JavaScript。

审计发现的问题可以分为四类。第一类是确定性功能错误，例如负屏幕坐标被拼成 `p.x--1920.0`、Flutter Manager 把业务字段 `state` 当成操作状态、运行时把超时写请求再次发送。第二类是并发与生命周期错误，例如 Codex 连接持有互斥锁时再次进入失败关闭流程、HTTP 处理线程异常后不归还连接计数。第三类是资源边界错误，例如 7z 没有单文件上限、RAR/TAR 在完整读入后才检查大小、Codex 计划增量缓存可以无限增长。第四类是平台行为缺口，例如 macOS Manager 生命周期函数恒返回失败、Linux 只把 X 根窗口视作一个显示器、X11 菜单用 `image_text8` 导致中文被替换成问号。

当前 Windows 主路径的高危问题已完成修复，QuickJS、HTTP、Codex、IPC、operation 协议和压缩包预算均补充了针对性回归测试。macOS 与 Linux 的纯代码路径已尽量在现有依赖内收敛，但本机是 Windows，不能把交叉编译等同于真机验证。Flutter 工程现已补齐 Linux/macOS runner，并将三平台 Rust、Flutter 和 release 打包纳入原生 CI；平台窗口行为仍以真机交互验收为准。

## 二、审计范围与判定方法

审计覆盖 `neurolings-engine`、`neurolings-pack`、`neurolings-platform`、`neurolings-runtime`、`neurolings-cli`、`neurolings-store` 和 Flutter Manager。对外契约以 CE v0.5.3 的 CLI 输出、IPC 消息、HTTP 路由、`.mascot` 包结构及桌宠行为状态为准；内部实现可以采用不同技术，但必须满足相同输入产生相同可观察结果，并且不能降低安全边界。

检查不是只看函数名是否存在。每条路径至少验证以下内容：输入缺失、显式 `null`、类型错误、超时、半包、短写、多字节 UTF-8、负坐标、多实例、线程异常、超限数据和进程关闭顺序。能在 Windows 上运行的路径使用真实二进制、真实 IPC 和真实桌宠模板验证；平台专属代码使用静态 API 核对、单元测试和目标平台 `cargo check`，并将无法在本机证明的部分明确标记为风险。

## 三、严重问题与修复状态

### 1. QuickJS 在负坐标显示器上生成非法表达式（P0，已修复）

根因位于 `crates/neurolings-engine/src/scripting/context.rs`。边界函数通过字符串插值生成 `Math.abs(p.x-{x})`。当 `x` 为 `-1920.0` 时，结果是 `Math.abs(p.x--1920.0)`，QuickJS 会把连续减号解释为非法语法。该错误只在显示器位于主屏左侧或下方时出现，因此单屏测试长期无法暴露。更严重的是，脚本错误此前被 `.ok()` 丢弃，日志只显示行为未命中，无法看到真正的 JS 异常。

修复方法是给插入的减数加括号，统一生成 `p.x-(-1920.0)` 和 `p.y-(-1080.0)`；求值改用 `CatchResultExt` 提取 QuickJS 异常；新增负坐标环境、完整 Default 行为列表和共享上下文交错 tick 测试。真实 IPC 连续召唤两只 Default 后，终端不再出现脚本警告。

### 2. 桌宠之间共享可变 JavaScript 全局对象（P0，已修复）

默认 `Factory` 曾让所有桌宠共用一个 QuickJS `Context`。任意模板只要执行 `Math = 7`、覆盖辅助函数或创建全局属性，就会改变其他桌宠随后的行为。状态快照虽然会在每次求值前替换 `mascot`，但无法恢复被改写的内建对象，因此这不是状态注入能够解决的问题。

修复后，默认工厂为每个 `Product` 创建独立 `ScriptContext`；只有调用方显式传入上下文时才保留共享语义，便于确定性测试和受控场景使用。回归测试让第一只桌宠覆盖 `Math`，确认第二只桌宠仍能调用 `Math.random`。

### 3. 桌宠包脚本可用无限循环卡死主循环（P0，已修复）

行为条件、变量表达式和普通脚本均来自可导入桌宠包。若只给 HTTP selector 设置执行上限，包内 `while(true){}` 仍会在运行时主线程无限执行，所有桌宠、IPC 和托盘事件都会停止响应。

所有包脚本入口现在共享 100ms 中断预算，自定义 selector 仍可使用调用方提供的更严格预算。超时通过 RAII 守卫安装和恢复 deadline，确保一次中断不会污染下一次求值。测试同时验证无限循环能够被中断，以及同一上下文随后仍可执行 `true`。这里的安全取舍是明确的：CE v0.5.3 没有为所有内部表达式设置预算，但 1.0.0 必须防止导入包永久占用主循环。

### 4. Codex 错误响应触发互斥锁自锁（P0，已修复）

`codex_appserver.rs` 的响应分发先取得 `client` 锁并从 pending 表删除请求，随后在错误响应或初始化失败时调用 `fail_closed`。`fail_closed` 又尝试取得同一个非重入 mutex，线程因此永久阻塞。审批请求超过上限时也有相同锁顺序问题。

修复方法不是增加超时兜底，而是缩短 guard 作用域：先在局部块中取出 pending 类型并释放锁，再进入失败关闭；审批超限分支在调用前显式释放 guard。新增两个带一秒完成窗口的并发回归测试，分别覆盖错误响应和审批上限。

### 5. Codex 中文长消息在字节边界切片时 panic（P1，已修复）

旧逻辑用 `message[cut..]` 保留末尾 128 KiB。Rust 字符串索引必须位于 UTF-8 字符边界；当 `cut` 落在中文字符中间时，处理线程会 panic。修复后的 `retain_tail_bytes` 从预算起点向后移动到合法字符边界，保证结果不超过预算且不破坏文本。回归输入以中文开头并跨过 128 KiB 限制，验证不会 panic。

### 6. Codex plan delta 缓存无界增长（P1，已修复）

计划增量按 thread、turn、item 组合键保存，但此前只有收到理想的 `item/completed` 才删除。异常断开、轮次失败、客户端发送大量不同 item id 或单项持续增量时，HashMap 会一直增长。

当前限制为单项 16 KiB、最多 64 项、总预算 128 KiB，键字段最多 256 个字符；缓存维护最近更新顺序，达到任一预算时淘汰较早条目。轮次开始、轮次完成、线程关闭、停止和失败关闭都会清空缓存。测试覆盖条目数、单项、总量、超长中文键及各类清理事件。

### 7. HTTP `%中` 触发字符串边界 panic（P0，已修复）

URL 解码循环以字节下标前进，却用 `&s[i + 1..i + 3]` 对 UTF-8 字符串切片。输入 `%中` 时，`i + 3` 落在“中”的编码内部，任何客户端都可以让连接处理线程 panic。

解码器现直接读取字节并手工解析十六进制，不再对原字符串按字节位置切片。有效百分号编码正常还原；截断或非法序列保留字面 `%`；编码结果不是 UTF-8 时使用替换字符，保证路由解析继续执行。测试覆盖中文编码、`%中`、孤立 `%`、`%2`、`%G0` 和 `%FF`。

### 8. HTTP 线程 panic 后连接名额永久泄漏（P1，已修复）

并发上限使用原子计数，但旧流程只在 `handle_connection` 正常返回后手动减一。线程 panic 会跳过该语句，重复触发后 32 个名额全部耗尽，服务即使仍在监听也不再处理请求。

当前以 `ConnectionSlot` 表示已占用名额，构造时原子增加，`Drop` 时归还。测试使用 `catch_unwind` 模拟处理函数 panic，确认计数回到零并能再次取得名额。

### 9. 慢命令超时后被 Manager 重放（P0，已修复）

运行时收到写命令后可能已经排入主线程，只是 HTTP 等待响应超时。Manager 若把 202 当作普通失败并再次发送原请求，安装、导入、删除、恢复组合等非幂等操作就会执行两次。

修复引入稳定 `operation_id`。只有服务端已经确认取得 operation id 的请求才能返回 202；尚未确认排队的超时返回 504。Manager 收到 202 后只轮询 `operation_status`，绝不重放写请求。完成结果保留 300 秒、最多 64 条，未知或过期 id 返回 404；Manager 默认轮询上限 270 秒，保证不会跨过服务端保留期。

### 10. operation 字段覆盖 GitHub 登录业务状态（P1，已修复）

完成响应曾把通用操作状态写到 `state`，而 Device Flow 同样使用 `state: pending/authorized` 表示登录阶段。通用包装层因此破坏业务结果，Store 页面可能把已授权状态读成 completed。

协议现使用独立字段 `operation_state`，并保留业务 `state`。回归测试分别覆盖 `pending` 与 `authorized`，确认包装完成结果后业务字段不变。

### 11. Manager 永久缓存空控制令牌（P0，已修复）

`RuntimeApi` 在对象构造时读取环境令牌。Flutter 状态对象可能先于运行时启动构造，此时令牌为空；即使运行时随后注入有效令牌，该 API 实例仍永久发送未鉴权请求。

默认实例现在在每次请求前动态读取当前内存令牌，测试专用的显式 `controlToken` 仍具有最高优先级。Flutter 回归测试覆盖“构造时无令牌、启动后令牌出现”的生命周期。

### 12. RAR/TAR/7z 解压预算检查过晚（P0，已修复，需持续模糊测试）

统一安全目标是：压缩包本身不超过 100 MiB、解压总量不超过 100 MiB、单文件不超过 16 MiB、文件数量不超过 4096。此前 7z 直接调用整包解压，没有单文件限制；TAR 和 RAR 把条目完整读入 `Vec` 后才累加总量。攻击者可以在错误返回前消耗大量内存和磁盘。

修复将预算前移到读取之前。TAR 使用 header size 预检，并通过 `take(remaining + 1)` 只多读一个探测字节；7z 使用自定义 extract callback，在默认写盘函数执行前检查声明尺寸、单项和总量；RAR 使用 `unpacked_size` 在 `extract_to()` 落盘前拒绝超限条目，并跳过目录项。所有路径共享溢出安全的预算函数，路径不安全而被跳过的文件仍计入条目数和声明尺寸预算。新增边界、超限、总量剩余、TAR/7z 实际归档和小文件成功解压测试。`unrar 0.5.8` 的公开 API 不提供可施加字节上限的流式写入回调，因此 RAR 在落盘后还会核对实际大小并删除不匹配文件；后续仍应使用畸形 RAR 语料做模糊测试，验证底层库始终遵守 header 声明尺寸。

### 13. IPC 把连接和响应共用一个短超时（P0，已修复）

CLI 启动运行时后，连接建立和业务执行具有不同时间尺度。旧实现用同一个 500ms 窗口覆盖两阶段，导致刚拉起的运行时、慢磁盘导入和系统调度抖动频繁误报。Windows 命名管道客户端还存在短写未检测、错误路径句柄关闭不完整的问题。

当前把连接、写入和读取超时分离；自动启动后的首次请求使用有限重试窗口；客户端与服务端都检测短写和零字节读取边界；`--stop` 按先确认响应、后等待进程退出的顺序执行。相关 CLI 集成测试和 platform 单元测试已通过。

### 14. Windows 可观察行为存在多处偏差（P1，已修复）

审计还修复了气泡 GDI 位图句柄误用、活动窗口 uid 切换、逐屏 DPI、窗口钳制与绘制偏移、双击与右键菜单、托盘恢复 Manager、默认模板和数据 id 稳定性、迁移注册表类型、更新安装后退出等问题。这些问题分散在 `bubble.rs`、`windows.rs`、`runtime`、`services.rs`、`templates.rs` 和 `migrate.rs`，但共同根因是只实现了“接口存在”，没有逐项验证 CE v0.5.3 的状态转换和平台坐标语义。

## 四、跨平台问题

### 1. macOS Manager 生命周期（P1，已修复代码，待真机）

`manager_window.rs` 曾让 macOS 使用通用 stub，全部生命周期方法恒为 false。当前已通过 `NSWorkspace.runningApplications` 定位 `neurolings_manager`，并用 `NSRunningApplication` 实现运行状态、应用隐藏、恢复和激活。Flutter Manager 每秒心跳同时上报 `windowManager.isVisible()`；运行时只转发合法布尔值，macOS 在三秒内优先采用窗口级心跳，超时后回退 `NSRunningApplication::isHidden`，成功 show/hide 时也同步缓存。纯函数测试覆盖命中与过期边界，最终仍需 macOS 真机验证 `orderOut`、激活和多屏组合。

### 2. macOS Manager 心跳坐标系（P1，已修复代码，待真机）

Rust 后端把虚拟桌面归一化到“所有屏幕最左、最上为零”的左上坐标系，而 `window_manager 0.4.3` 的 macOS 插件用主屏高度翻转 Y，且不减虚拟桌面的最左坐标。左侧或上方外接屏会导致 Manager 矩形落入错误环境，随后召唤到错误屏幕。运行时现在依据 `EnvironmentSet` 的屏幕矩形规范化 macOS 心跳，纯函数测试覆盖左侧和上方显示器布局；最终仍需双屏 macOS 真机验证。

### 3. Linux 多显示器枚举（P1，受依赖配置约束）

当前 `x11rb` 只启用了 `xfixes` 与 `shape`，没有 RandR/Xinerama feature。代码枚举多个 X root，但常见 Xinerama/RandR 桌面只有一个 root，因此最终仍把整个虚拟桌面视作一块屏，工作区、召唤落点和屏幕钳制都会失去逐屏语义。可靠修复需要启用 RandR 并读取 active CRTC/output，或接入已有的屏幕枚举库；这项平台行为修复不属于当前构建收敛范围，不能伪装成已解决。

### 4. Linux 中文菜单（P1，现有 X Core Text 能力不足）

弹出菜单将非 ASCII 字符替换为 `?` 后调用 `image_text8`。直接改成 UTF-8 字节仍不正确，因为 X Core Text 不是 UTF-8；`image_text16` 也依赖服务器端双字节字体，不能保证中文字体和 Unicode 映射。可靠方案是使用 Pango/Cairo/Xft 或让 GTK/Flutter 承载菜单，这同样涉及新依赖和平台构建配置。当前构建修复保留这项明确风险，不能声称中文菜单已对齐。

### 5. Flutter Linux/macOS runner 缺失（P0，已修复工程配置）

`manager/.metadata` 现已登记 Windows、Linux 和 macOS，两个平台 runner 也已纳入版本控制。CI 在三个原生宿主上分别执行 Rust 门禁、Flutter analyze/test 和 release build；发布工作流把 Rust runtime、CLI 与 Manager 合并为对应平台产物。macOS App Sandbox 已按本地控制面和子进程需求关闭，加入 Rust 二进制后执行临时重签名与严格校验。正式分发仍需维护者配置 Apple Developer 签名、公证及各平台真机验收。

## 五、验证证据

本报告只记录当前最终工作树实际执行过的结果，不把推测或更早工作树的结果写成通过。最终证据如下：

- `cargo test --workspace --locked`：194/194 通过。分组为 CLI 25、common 10、engine 24、pack 37、platform 12、runtime 75、store 11；全部 doctest 也通过且没有测试被忽略。
- `cargo check --workspace --all-targets --locked`：通过；`cargo clippy --workspace --all-targets --locked -- -D warnings`：通过；`cargo fmt --all -- --check`：通过。
- `cargo build --workspace --locked`：通过，随后使用该次最终构建产物执行运行时验证。
- `flutter analyze --no-pub`：0 问题；`flutter test --no-pub`：5/5 通过。覆盖 operation 只轮询、动态控制令牌、后台失败透传、窗口可见性心跳与 Manager 基础渲染。
- `cargo check -p neurolings-platform --target x86_64-apple-darwin --locked` 与 Linux 对应目标：通过。runtime 的 macOS 完整交叉构建仍受本机缺少 macOS C/C++ 工具链限制，不能据此替代真机构建。
- Default、Cerber、Eviling、Neuron、Tuteling、Vedaling、Weuron 分别运行 300 tick，七个最终构建进程均报告加载 7 个模板并完成 300/300，总计 2100 tick。
- 真实 IPC 连续召唤两只 Default，得到不同运行时 id 1/2 和标签 101/102；列表同时返回两只，关闭 101 后只剩 102，`--close-all` 后为空。再次召唤标签 103 后，`--stop` 返回 `stopped: true`。
- IPC 验证结束后 NeurolingsCE/Manager 残留进程为 0；最新会话日志中 QuickJS、脚本 panic 关键字命中数为 0。
- `git diff --check`：通过；新增差异中的禁用措辞扫描：0 命中。

## 六、有意保留的差异

有些差异不是遗漏，而是为消除未定义行为或提高安全性作出的明确选择：

1. 脚本求值错误记录一次警告并回退，不因单个包表达式错误直接关闭桌宠；无限循环仍会被强制中断。
2. `is_num` 要求结果是有限数，不复制 `strtod` 对正负溢出的不对称边缘行为。
3. 对外默认模板名统一为 `Default`，同时兼容 `@` 与 `Default Mascot` 别名。
4. breed/transform 子代朝向使用确定值，不复制未初始化内存产生的随机结果。
5. 无效 Codex 转发配置静默跳过，避免未启用集成时持续刷日志；配置存在但执行失败仍记录警告。

这些选择必须保留测试和文档，避免后续维护者为了表面一致而重新引入崩溃、挂起或未定义行为。

## 七、剩余风险与发布建议

当前不应把“三平台均可构建”表述为“三平台行为全部完成”。Linux/macOS runner 与原生构建门禁已经补齐，但它们不能证明窗口层级、透明命中、菜单字体、托盘、辅助功能权限和多屏坐标符合真实桌面环境。

建议先以三平台 CI 和联合发布工作流固定可重复的构建证据，再补齐 RandR 屏幕枚举和 Unicode 菜单渲染，最后分别在 Windows 混合 DPI、Linux X11/XWayland、macOS 双屏与辅助功能权限开关两种状态下执行真实桌宠、托盘、Manager 显隐、活动窗口攀附和关机恢复测试。

本轮修复已经消除了已定位的 Windows 主路径 P0/P1 缺陷，但“全量修复”在工程意义上还要求上述平台阻塞被解除。报告将这些边界明确保留，是为了避免用交叉编译成功替代用户可运行的证据。
