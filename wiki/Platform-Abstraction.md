# 平台抽象层 / Platform Abstraction

本页面详细介绍 NeurolingsCE 的跨平台实现机制。

> This page details the cross-platform implementation mechanism of NeurolingsCE.

## 概览 / Overview

平台抽象层位于 `src/platform/Platform/`，通过统一的 `Platform::` 命名空间封装各操作系统的差异。每个平台子目录独立编译，产出静态库 `Platform.a`。

```
src/platform/Platform/
├── Platform.hpp              # 公共 API
├── ActiveWindow.hpp          # 窗口数据类
├── ActiveWindowObserver.hpp  # 窗口观察器接口
├── Makefile                  # 按平台委托到子目录
├── Windows/                  # Win32 API 实现
├── Linux/                    # X11 + DBus + KDE/GNOME 扩展
├── macOS/                    # Objective-C (AppKit)
└── Stub/                     # 空实现（不支持的平台）
```

## 公共 API / Public API

### Platform 命名空间

```cpp
namespace Platform {
    void initialize(int argc, char **argv);
    void showOnAllDesktops(QWidget *widget);
    bool useWindowMasks();
}
```

| 函数 | 说明 |
|------|------|
| `initialize()` | 在 `QApplication` 创建前进行平台特定初始化 |
| `showOnAllDesktops()` | 确保窗口在所有虚拟桌面上可见 |
| `useWindowMasks()` | 是否使用 `QRegion` 窗口遮罩（仅 Linux 返回 `true`） |

### ActiveWindow 数据类

```cpp
class ActiveWindow {
public:
    bool available;     // 是否有有效的前台窗口
    QString uid;        // 窗口唯一标识
    long pid;           // 所属进程 PID
    double x, y;        // 窗口位置
    double width, height; // 窗口尺寸
};
```

### ActiveWindowObserver 观察器

```cpp
class ActiveWindowObserver : public QObject {
public:
    ActiveWindowObserver();
    int tickFrequency();        // 建议的轮询间隔（毫秒）
    void tick();                // 轮询一次前台窗口
    ActiveWindow getActiveWindow(); // 获取当前前台窗口
    ~ActiveWindowObserver();
private:
    PrivateActiveWindowObserver *m_private; // 平台私有实现
};
```

> **设计模式 / Design Pattern**: 使用 Pimpl（Pointer to Implementation）模式。`PrivateActiveWindowObserver` 在每个平台子目录中定义，公共头文件只有前向声明。

## 平台实现 / Platform Implementations

### Windows

**文件**: `src/platform/Platform/Windows/`

| 功能 | 实现方式 |
|------|---------|
| 初始化 | 重定向 stdout/stderr 到 `shijima_stdout.txt` / `shijima_stderr.txt` |
| 多桌面显示 | 使用 `WS_EX_TOOLWINDOW` 窗口样式，隐藏看板娘在任务栏和 Alt-Tab 中的显示 |
| 窗口遮罩 | 不使用（`useWindowMasks()` 返回 `false`） |
| 窗口追踪 | Win32 `GetForegroundWindow()` API |

**关键点:**
- 窗口追踪开箱即用，无需额外配置
- 使用 `HWND` 相关 API 管理窗口属性

### Linux（最复杂 / Most Complex）

**文件**: `src/platform/Platform/Linux/`

| 文件 | 职责 |
|------|------|
| `Platform.cc` | 初始化：强制 X11、注册信号处理 |
| `ActiveWindowObserver.cc` | 前台窗口轮询逻辑 |
| `PrivateActiveWindowObserver.cc` | KDE/GNOME 后端选择与管理 |
| `KWin.cc` | KDE Plasma KWin 脚本交互 |
| `KDEWindowObserverBackend.cc` | KDE 窗口追踪后端 |
| `GNOME.cc` | GNOME Shell 扩展交互 |
| `GNOMEWindowObserverBackend.cc` | GNOME 窗口追踪后端 |
| `DBus.cc` | DBus 通信封装 |
| `ExtensionFile.cc` | 管理 KWin/GNOME 扩展的安装 |

**关键设计决策:**

1. **强制 X11**: 设置 `WAYLAND_DISPLAY=""` 禁用 Wayland。看板娘需要在桌面任意位置定位，这在原生 Wayland 下不可行。

2. **双后端窗口追踪**:
   - **KDE** — 通过 KWin Script 获取前台窗口信息（对用户透明）
   - **GNOME** — 通过 Shell Extension 获取（首次运行后需要重新登录）

3. **DBus 通信**: 两种后端都通过 DBus 与桌面环境通信。

4. **窗口遮罩**: Linux 使用 `QRegion` 作为窗口 mask（`useWindowMasks()` 返回 `true`），实现精确的点击穿透——只有看板娘图像的非透明区域可以接收鼠标事件。

5. **信号处理**: 使用 `socketpair()` 实现 SIGTERM 的优雅关闭。

### macOS

**文件**: `src/platform/Platform/macOS/`

| 文件 | 说明 |
|------|------|
| `Platform.mm` | Objective-C++ 初始化 |
| `ActiveWindowObserver.cc` | 观察器 C++ 接口 |
| `PrivateActiveWindowObserver.mm` | AppKit/Accessibility API 实现 |

**关键点:**
- 使用 Objective-C++ (`.mm` 文件) 与 AppKit 交互
- 窗口追踪需要 **辅助功能（Accessibility）权限**
- 最低要求 macOS 13 (Ventura)
- 链接 `-lobjc -framework AppKit -framework ApplicationServices`

### Stub（空实现）

**文件**: `src/platform/Platform/Stub/`

所有函数的空实现，用于不支持的平台。确保代码可以编译通过，但窗口追踪等功能不可用。

## 构建选择 / Build Selection

### CMake

```cmake
if(WIN32)
    # 编译 Windows/ 下的源文件
elseif(APPLE)
    # 编译 macOS/ 下的源文件
elseif(UNIX)
    # 编译 Linux/ 下的源文件
endif()
```

### Make

在 `common.mk` 中自动检测 `BUILD_PLATFORM` 变量，`Platform/Makefile` 据此委托到对应子目录。

## 扩展指南 / Extending

如需添加新平台支持：

1. 在 `src/platform/Platform/` 下创建新子目录（如 `FreeBSD/`）
2. 实现以下文件:
   - `Platform.cc` — 实现 `initialize()`, `showOnAllDesktops()`, `useWindowMasks()`
   - `ActiveWindowObserver.cc` — 实现观察器接口
   - `PrivateActiveWindowObserver.cc` + `.hpp` — 私有实现
3. 更新 `CMakeLists.txt` 和 `Platform/Makefile` 的平台判断逻辑

> **规则 / Rules**: 每个平台子目录必须完全自包含。不要跨平台目录 `#include`。

## 相关页面 / Related Pages

- [架构概览](Architecture) — 整体架构
- [构建指南](Building) — 不同平台的构建配置
