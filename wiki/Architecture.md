# 架构概览 / Architecture

本页面介绍 NeurolingsCE 的整体代码架构和核心组件。

> This page provides an overview of the NeurolingsCE codebase architecture and core components.

## 目录结构 / Directory Structure

```
NeurolingsCE/
├── src/app/                  # Qt 应用层（14 个 .cc 源文件）
├── src/platform/Platform/    # 平台抽象层（Windows/Linux/macOS）
├── include/shijima-qt/       # 公共头文件（与 src/app/ 一一对应）
├── libshijima/               # [子模块] 核心看板娘模拟引擎
├── libshimejifinder/         # [子模块] 资源包导入解压
├── cpp-httplib/              # [子模块] HTTP 服务器（header-only）
├── miniz/                    # 轻量级 ZIP 库（用于 SimpleZipImporter）
├── cmake/                    # CMake 辅助脚本
├── translations/             # i18n 翻译文件
├── src/assets/               # 内置默认看板娘精灵图 + XML
├── src/packaging/            # 桌面入口、图标、.app 骨架
├── src/tools/                # Shell 辅助脚本
├── src/docs/                 # HTTP API 文档
├── src/resources/            # Windows 资源文件（.rc）+ Qt 资源（.qrc）
├── licenses/                 # 第三方许可证文本（构建时嵌入）
├── dev-docker/               # Fedora Docker 镜像（Windows 交叉编译）
└── .github/workflows/        # CI 工作流
```

## 核心类图 / Core Class Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                          main.cc                                 │
│  QApplication → 单实例检查 → ShijimaManager::defaultManager()   │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                     ShijimaManager                               │
│  (PlatformWidget<QMainWindow> 单例)                              │
│                                                                   │
│  职责 / Responsibilities:                                        │
│  ├── 管理看板娘生命周期（生成、销毁、tick 循环）                  │
│  ├── 管理已加载的看板娘资源包 (MascotData)                       │
│  ├── 同步多屏幕环境信息 (environment)                            │
│  ├── 处理拖放导入                                                │
│  ├── 语言切换                                                    │
│  └── 协调 HTTP API 和窗口观察器                                  │
│                                                                   │
│  关键成员 / Key Members:                                         │
│  ├── m_mascots: list<ShijimaWidget*>    ← 活跃的看板娘实例       │
│  ├── m_loadedMascots: QMap<名称, MascotData*>                    │
│  ├── m_httpApi: ShijimaHttpApi          ← HTTP REST API          │
│  ├── m_windowObserver: ActiveWindowObserver  ← 窗口追踪          │
│  ├── m_factory: shijima::mascot::factory     ← 看板娘工厂        │
│  └── m_env: QMap<QScreen*, environment>      ← 每屏幕环境        │
└──────┬──────────┬──────────┬──────────┬────────────────────────┘
       │          │          │          │
       ▼          ▼          ▼          ▼
┌────────────┐ ┌────────┐ ┌──────────┐ ┌────────────────────┐
│ShijimaWidget│ │MascotData│ │ShijimaHttpApi│ │ActiveWindowObserver│
│(看板娘窗口) │ │(资源数据) │ │(HTTP API)    │ │(窗口追踪)          │
└────────────┘ └────────┘ └──────────┘ └────────────────────┘
```

## 核心组件详解 / Core Components

### 1. ShijimaManager（看板娘管理器）

**文件**: `src/app/ShijimaManager.cc` + `include/shijima-qt/ShijimaManager.hpp`

全局单例，通过 `ShijimaManager::defaultManager()` 访问。继承自 `PlatformWidget<QMainWindow>`。

**核心功能 / Key Functions:**

| 方法 | 说明 |
|------|------|
| `spawn(name)` | 生成指定名称的看板娘实例 |
| `killAll()` | 销毁所有看板娘 |
| `killAllButOne(widget/name)` | 保留一只，销毁其余 |
| `updateEnvironment()` | 同步所有屏幕的环境信息（位置、大小、工作区域） |
| `tick()` | 主循环回调，驱动所有看板娘的帧更新 |
| `import(path)` | 导入看板娘资源包 |
| `onTickSync(callback)` | 线程安全回调（HTTP API 线程使用） |

**Tick 循环 / Tick Loop:**
- 以 **25 FPS** 运行（硬编码在 libshijima 设计中）
- 通过 `QBasicTimer` 的 `timerEvent()` 驱动
- 每帧：更新环境 → tick 所有看板娘 → 处理跨线程回调

**线程安全 / Thread Safety:**
- `std::mutex` + `std::condition_variable` 保护跨线程操作
- HTTP API 在独立线程运行，通过 `onTickSync()` 安全地在主线程执行回调

### 2. ShijimaWidget（看板娘窗口）

**文件**: `src/app/ShijimaWidget.cc` + `include/shijima-qt/ShijimaWidget.hpp`

继承自 `PlatformWidget<QWidget>`。每个看板娘实例对应一个透明无边框窗口。

**核心功能:**

| 功能 | 实现 |
|------|------|
| 渲染 | `paintEvent()` — 绘制当前帧精灵图，支持镜像 |
| 鼠标交互 | `mousePressEvent()` / `mouseReleaseEvent()` — 拖拽和右键菜单 |
| 帧更新 | `tick()` — 推进 libshijima 模拟状态，更新位置和图像 |
| Hit 测试 | `pointInside()` — 判断屏幕坐标是否在看板娘图像内 |
| 检查器 | `showInspector()` — 打开调试信息对话框 |

**Fall-Through 机制:**
- 追踪看板娘坠落距离（`m_fallTracking`）
- 当坠落超过 700 像素时启用 "穿越模式"（`m_fallThroughMode`），看板娘会落到屏幕最底部而非停在任务栏

**缩放 / Scaling:**
- `m_drawScale` — 绘制缩放比例
- 用户可通过管理器设置自定义缩放

### 3. MascotData（看板娘数据）

**文件**: `src/app/MascotData.cc` + `include/shijima-qt/MascotData.hpp`

封装单个看板娘资源包的数据：

| 属性 | 说明 |
|------|------|
| `name` | 看板娘名称（目录名） |
| `path` | 资源包路径 |
| `behaviorsXML` | 行为定义 XML |
| `actionsXML` | 动作定义 XML |
| `imgRoot` | 图片根目录 |
| `preview` | 预览图标 |
| `deletable` | 是否可删除（默认看板娘不可删除） |
| `id` | 唯一整数 ID |

### 4. AssetLoader（资源加载器）

**文件**: `src/app/AssetLoader.cc` + `include/shijima-qt/AssetLoader.hpp`

单例模式，管理看板娘精灵图的加载和缓存。

| 方法 | 说明 |
|------|------|
| `loadAsset(path)` | 加载并缓存图片资源，返回 `Asset` 引用 |
| `unloadAssets(root)` | 卸载指定根目录下的所有缓存资源 |

### 5. Asset（精灵图资源）

**文件**: `src/app/Asset.cc` + `include/shijima-qt/Asset.hpp`

封装单个精灵图帧：
- `m_image` / `m_mirrored` — 原始和镜像图像
- `m_offset` — 裁剪偏移
- `m_mask` / `m_mirroredMask` — Linux 上用于窗口区域遮罩的 `QBitmap`

### 6. ShijimaHttpApi（HTTP API）

**文件**: `src/app/ShijimaHttpApi.cc` + `include/shijima-qt/ShijimaHttpApi.hpp`

在独立线程启动 HTTP 服务器（默认 `127.0.0.1:32456`），提供 REST API 接口。

详细 API 文档见 [HTTP API](HTTP-API)。

### 7. ShijimaContextMenu（右键菜单）

**文件**: `src/app/ShijimaContextMenu.cc` + `include/shijima-qt/ShijimaContextMenu.hpp`

右键点击看板娘时弹出的上下文菜单，提供暂停、检查、分裂、关闭等操作。

### 8. SimpleZipImporter（ZIP 导入器）

**文件**: `src/app/SimpleZipImporter.cc` + `include/shijima-qt/SimpleZipImporter.hpp`

轻量级 ZIP 看板娘包导入器，基于 miniz。在 MSVC 构建中用于替代 libshimejifinder。

支持的布局：
1. **根级** — `actions.xml` + `behaviors.xml` + `img/` 直接在 ZIP 根目录
2. **Shimeji-ee** — 包含 `shimeji-ee.jar` + `conf/` + `img/<name>/`
3. **子目录** — `<name>/conf/actions.xml` + `img/`
4. **纯图片** — 只有 `shime1.png` ~ `shime46.png`（使用内置默认 XML）

### 9. SoundEffectManager（音效管理器）

**文件**: `src/app/SoundEffectManager.cc` + `include/shijima-qt/SoundEffectManager.hpp`

可选功能（需要 `SHIJIMA_USE_QTMULTIMEDIA=1`），管理看板娘的音效播放。

### 10. PlatformWidget（平台窗口模板）

**文件**: `include/shijima-qt/PlatformWidget.hpp`

CRTP 模板类，为 `QWidget` / `QMainWindow` 添加跨平台行为：
- `ShowOnAllDesktops` — 确保窗口在所有虚拟桌面上可见
- 通过 `QTimer` 延迟调用 `Platform::showOnAllDesktops()`

```cpp
// 用法
class ShijimaManager : public PlatformWidget<QMainWindow> { ... };
class ShijimaWidget : public PlatformWidget<QWidget> { ... };
```

## 数据流 / Data Flow

### 启动流程 / Startup Flow

```
main()
  │
  ├── argc > 1? → shijimaRunCli()  // CLI 模式
  │
  ├── Platform::initialize()        // 平台初始化
  ├── QApplication 创建
  │
  ├── HTTP ping 127.0.0.1:32456    // 单实例检查
  │   ├── 响应成功 → 报错退出
  │   └── 无响应 → 继续
  │
  ├── ShijimaManager::defaultManager()->show()
  │   ├── 加载默认看板娘 (DefaultMascot.cc)
  │   ├── 扫描 mascots/ 目录加载已有资源包
  │   ├── 启动 HTTP API 服务器
  │   ├── 启动窗口观察器定时器
  │   └── 启动 tick 定时器 (25 FPS)
  │
  └── app.exec()  // Qt 事件循环
```

### Tick 循环 / Tick Loop

```
timerEvent() (每 40ms = 25 FPS)
  │
  ├── updateEnvironment()           // 更新屏幕/窗口信息
  │   └── m_windowObserver.tick()   // 轮询前台窗口
  │
  ├── for each ShijimaWidget:
  │   └── widget->tick()
  │       ├── m_mascot->tick()      // libshijima 状态推进
  │       ├── updateOffsets()       // 计算绘制位置
  │       └── repaint()            // 触发 paintEvent
  │
  └── 处理 tickCallbacks           // HTTP API 的线程安全回调
      └── m_tickCallbackCompletion.notify_all()
```

### 资源包导入流程 / Mascot Import Flow

```
用户拖拽 .zip → dropEvent()
  │
  ├── SimpleZipImporter::import()    // 或 libshimejifinder
  │   ├── 解压 ZIP
  │   ├── 识别资源包布局
  │   └── 复制到 mascots/<name>/
  │
  ├── loadData(MascotData*)          // 解析 XML 注册到工厂
  │   └── m_factory.load_template()  // libshijima 加载模板
  │
  └── refreshListWidget()            // 刷新 UI 列表
```

## 子模块关系 / Submodule Relations

```
NeurolingsCE (Qt App)
  │
  ├── libshijima (只读依赖 / Read-only)
  │   ├── 看板娘行为模拟引擎
  │   ├── XML 解析 (rapidxml)
  │   ├── JavaScript 脚本 (duktape)
  │   └── shijima::mascot::manager / factory / environment
  │
  ├── libshimejifinder (只读依赖 / Read-only)
  │   ├── 资源包归档解压
  │   └── 依赖 libunarr + libarchive
  │
  └── cpp-httplib (只读依赖 / Read-only)
      └── HTTP 服务器/客户端 (header-only)
```

> **注意 / Note**: libshijima **不接受**外部贡献（其 README 中明确声明）。作为只读依赖使用。

## 条件编译 / Conditional Compilation

| 宏 / Macro | 说明 |
|------------|------|
| `SHIJIMA_USE_QTMULTIMEDIA` | `=1` 启用音效，`=0` 禁用 |
| `SHIJIMA_WITH_SHIMEJIFINDER` | `=1` 启用 libshimejifinder 导入，`=0` 禁用 |
| `NEUROLINGSCE_VERSION` | 版本号字符串（如 `"0.1.0"`） |
| `WIN32` | Windows 平台 |
| `__linux__` | Linux 平台 |
| `__APPLE__` | macOS 平台 |
| `NOMINMAX` / `WIN32_LEAN_AND_MEAN` | Windows 头文件精简 |

## 相关页面 / Related Pages

- [平台抽象层](Platform-Abstraction) — 详细的跨平台实现
- [HTTP API](HTTP-API) — REST API 接口文档
- [构建指南](Building) — 编译配置详解
