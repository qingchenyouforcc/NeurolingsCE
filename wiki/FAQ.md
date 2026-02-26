# 常见问题 / FAQ

常见问题及解答。

> Frequently asked questions and answers.

---

## 通用问题 / General

### Q: NeurolingsCE 和 Shijima-Qt 有什么关系？

NeurolingsCE 是 [Shijima-Qt](https://github.com/pixelomer/Shijima-Qt) 的 fork（分支），在其基础上进行了大量修改和功能增强。核心模拟引擎 libshijima 仍来自上游。

> NeurolingsCE is a fork of Shijima-Qt with extensive modifications. The core simulation engine (libshijima) remains from upstream.

### Q: NeurolingsCE 支持哪些看板娘格式？

兼容 **Shimeji-ee** 格式的资源包。只要包含 `actions.xml`、`behaviors.xml` 和 `img/shime*.png` 精灵图的资源包就能使用。

### Q: 可以同时运行多个看板娘吗？

可以。你可以生成同一资源包的多个实例，也可以同时运行不同资源包的看板娘。

### Q: 看板娘的帧率是多少？

固定 **25 FPS**，这是 libshijima 的设计决定，硬编码在引擎中。

### Q: 能同时运行多个 NeurolingsCE 实例吗？

不能。NeurolingsCE 使用单实例机制——启动时会检查 `127.0.0.1:32456` 端口。如果已有实例运行，新进程会提示并退出。

---

## Windows 问题 / Windows Issues

### Q: 启动时提示 "缺少 Qt6Guid.dll" 或其他 DLL

**原因**: Qt 运行时 DLL 未部署到程序目录。

**解决方案**:
1. 使用 CMake 构建时，确保 `windeployqt` 可用（CMake 会自动调用）
2. 或手动运行：`windeployqt NeurolingsCE.exe`
3. 或将 Qt 的 `bin` 目录添加到系统 PATH

### Q: 编译时提示 "Detected 32-bit MSVC toolchain"

**原因**: 使用了 32 位的 MSVC 工具链。

**解决方案**: 项目仅支持 x64。请使用 "x64 Native Tools Command Prompt" 或在 Visual Studio 中选择 x64 平台。

### Q: 看板娘出现在任务栏 / Alt-Tab 中

正常情况下看板娘使用 `WS_EX_TOOLWINDOW` 窗口样式，不会出现在任务栏和 Alt-Tab 中。如果出现此问题，可能是某些第三方工具干扰了窗口属性。

---

## Linux 问题 / Linux Issues

### Q: GNOME 上看板娘无法追踪前台窗口

**原因**: GNOME 需要安装 Shell 扩展来获取前台窗口信息。

**解决方案**:
1. 首次运行 NeurolingsCE 后，程序会自动安装扩展
2. **需要重新登录**（注销再登录）以重启 GNOME Shell
3. 重新启动 NeurolingsCE

### Q: KDE 上看板娘窗口追踪不工作

KDE 窗口追踪通过 KWin 脚本实现。确保：
1. 使用的是 KDE Plasma 6
2. KWin 脚本服务正常运行（DBus 可达）

### Q: 在 Wayland 下运行出问题

NeurolingsCE **强制使用 X11**（设置 `WAYLAND_DISPLAY=""`），因为看板娘需要在桌面任意位置定位，这在原生 Wayland 下不可行。

如果你使用 Wayland，程序会自动通过 XWayland 运行。确保 XWayland 已安装。

### Q: 其他桌面环境（如 XFCE、i3）能用吗？

可以运行，但窗口追踪功能不可用。看板娘仍可在桌面上活动，只是无法与前台窗口互动。

---

## macOS 问题 / macOS Issues

### Q: 看板娘无法追踪前台窗口

**原因**: 缺少辅助功能权限。

**解决方案**:
1. 打开 **系统设置 → 隐私与安全 → 辅助功能**
2. 将 NeurolingsCE（或 Terminal，如果从终端运行）添加到列表中并启用
3. 重启应用

### Q: 最低支持的 macOS 版本是什么？

macOS 13 (Ventura)。

---

## 构建问题 / Build Issues

### Q: CMake 找不到 Qt6

设置 Qt 路径：
```bash
cmake -B build -DQt6_DIR=/path/to/Qt/6.8.x/gcc_64/lib/cmake/Qt6
# 或
cmake -B build -DCMAKE_PREFIX_PATH=/path/to/Qt/6.8.x/gcc_64
# 或设置环境变量
export QTDIR=/path/to/Qt/6.8.x/gcc_64
```

### Q: 子模块克隆失败

```bash
# 确保递归初始化
git submodule update --init --recursive

# 如果 SSH 不通，可能需要改用 HTTPS
# 编辑 .gitmodules 将 git@ 改为 https://
```

### Q: libshimejifinder 在 MSVC 上编译失败

这是预期行为。MSVC 构建默认关闭 libshimejifinder（`SHIJIMA_WITH_SHIMEJIFINDER=OFF`）。程序使用内置的 `SimpleZipImporter` 作为替代，支持 `.zip` 格式的导入。

### Q: Qt LinguistTools 找不到

确保安装了 Qt 的 LinguistTools 模块：
- **Linux**: `sudo apt install qt6-tools-dev qt6-l10n-tools`
- **macOS**: MacPorts Qt6 默认包含
- **MSVC**: 确保 Qt 安装时选择了 "Qt Linguist" 组件

如果确实缺少，构建会发出警告但不会失败，只是翻译不会被编译和嵌入。

---

## HTTP API 问题 / HTTP API Issues

### Q: API 无响应

确保：
1. NeurolingsCE 正在运行
2. 访问 `http://127.0.0.1:32456/shijima/api/v1/ping` 应返回响应
3. 端口 32456 没有被其他程序占用

### Q: 端口被占用

如果端口 32456 已被其他程序占用，NeurolingsCE 可能无法正常启动。需要先关闭占用该端口的程序。

> **当前限制 / Current Limitation**: 端口号目前是硬编码的，不可配置。

---

## 相关页面 / Related Pages

- [快速开始](Getting-Started)
- [构建指南](Building)
- [HTTP API](HTTP-API)
- [平台抽象层](Platform-Abstraction)
