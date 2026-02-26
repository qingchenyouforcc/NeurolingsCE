# 构建指南 / Building

本页面详细说明如何从源码编译 NeurolingsCE。

> This page provides detailed instructions for building NeurolingsCE from source.

## 前置依赖 / Prerequisites

| 依赖 | 版本要求 | 说明 |
|------|---------|------|
| C++ 编译器 | C++17 | MSVC 2022 / GCC / Clang |
| Qt | 6.8+ | Core, Gui, Widgets, Concurrent, LinguistTools |
| CMake | 3.21+ | Windows/MSVC 构建系统 |
| Make | - | Linux/macOS 构建系统 |
| Git | - | 需要递归克隆子模块 |

### Qt 组件说明 / Qt Components

| 组件 | 用途 | 必须 |
|------|------|------|
| Qt6::Core | 基础库 | 是 |
| Qt6::Gui | 图形渲染 | 是 |
| Qt6::Widgets | UI 控件 | 是 |
| Qt6::Concurrent | 异步加载 | 是 |
| Qt6::LinguistTools | 翻译编译（`.ts` → `.qm`） | 是 |
| Qt6::Multimedia | 音效播放 | 可选 |

## 获取源码 / Get Source

```bash
git clone https://github.com/qingchenyouforcc/NeurolingsCE.git
cd NeurolingsCE
git submodule update --init --recursive
```

> **子模块 / Submodules**: 项目包含三个 git 子模块（libshijima, libshimejifinder, cpp-httplib），必须完成初始化才能编译。

## 双构建系统 / Dual Build Systems

NeurolingsCE 维护两套构建系统：

| 方面 | CMake | Make |
|------|-------|------|
| 主要用途 | Windows MSVC / Visual Studio | MinGW / Linux / macOS |
| Qt 发现 | `find_package(Qt6)` | `pkg-config` / Framework 路径 |
| 配置方式 | `CMakeSettings.json` (VS) / CMake 变量 | `CONFIG=debug\|release` 环境变量 |
| libshimejifinder | 默认关闭（MSVC） | 始终编译 |
| Conda Qt 修复 | 自动检测，强制 `/MD` 运行时 | 不适用 |

---

## Windows（MSVC + CMake）

这是 Windows 上的**推荐构建方式**。

### 方式一：命令行 / Command Line

```bash
# 配置（指定 Qt 路径）
cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DQt6_DIR=D:/Qt/6.8.3/msvc2022_64/lib/cmake/Qt6

# 编译
cmake --build build
```

> **重要 / Important**: 必须使用 **x64 工具链**。项目不支持 32 位 MSVC，会直接报错。

### 方式二：Visual Studio

1. 用 Visual Studio 2022 打开项目根目录
2. VS 会自动加载 `CMakeSettings.json`，已预配置 `x64-Debug` 和 `x64-Release`
3. 需要调整 `CMakeSettings.json` 中的 `Qt6_DIR` 为你本地的 Qt 路径
4. 选择配置后直接构建

### Qt 路径配置 / Qt Path Configuration

CMake 会尝试以下方式查找 Qt：

1. `-DQt6_DIR=<path>` — 直接指定 Qt6 CMake 配置路径
2. `-DCMAKE_PREFIX_PATH=<path>` — 指定 Qt 安装前缀
3. 环境变量 `QTDIR` — 自动设为 `CMAKE_PREFIX_PATH`

### windeployqt

CMake 配置会自动运行 `windeployqt`，将 Qt 运行时 DLL 复制到输出目录。如果缺少 `windeployqt`，会在构建时发出警告。

### CMake 选项 / CMake Options

| 选项 | 默认值 | 说明 |
|------|--------|------|
| `SHIJIMA_USE_QTMULTIMEDIA` | `ON` | 启用 Qt Multimedia 音效支持 |
| `SHIJIMA_BUILD_EXAMPLES` | `OFF` | 编译 libshijima SDL2 示例 |
| `SHIJIMA_WITH_LIBSHIMEJIFINDER` | `OFF` (MSVC) | 编译 libshimejifinder |
| `SHIJIMA_WITH_SHIMEJIFINDER` | `OFF` (MSVC) / `ON` (其他) | 启用归档包导入 |
| `SHIJIMA_WITH_DEFAULT_MASCOT` | `ON` | 嵌入默认看板娘资源（不可关闭） |
| `SHIJIMA_WITH_LICENSES_TEXT` | `ON` | 嵌入许可证文本（不可关闭） |

> **注意 / Note**: `SHIJIMA_WITH_DEFAULT_MASCOT=OFF` 和 `SHIJIMA_WITH_LICENSES_TEXT=OFF` 当前**不受支持**，会直接报错。

### Conda Qt 兼容 / Conda Qt Compatibility

如果你使用 Conda 提供的 Qt（如 miniconda/anaconda），CMake 会自动检测并将 MSVC 运行时强制为 `/MD`（Release CRT），避免 Debug/Release CRT 冲突。

---

## Windows（MinGW 交叉编译 via Docker）

适用于 CI 环境或偏好 GCC 的开发者。使用 Fedora-based Docker 镜像进行交叉编译。

```bash
# 构建 Docker 镜像
docker build -t neurolingsce-dev dev-docker

# 编译（Release）
docker run -e CONFIG=release --rm -v "$(pwd)":/work \
  neurolingsce-dev bash -c 'mingw64-make -j$(nproc)'

# 编译（Debug）
docker run -e CONFIG=debug --rm -v "$(pwd)":/work \
  neurolingsce-dev bash -c 'mingw64-make -j$(nproc)'
```

产物位于 `publish/Windows/<config>/`。

---

## Linux

### 安装依赖 / Install Dependencies

**Ubuntu/Debian:**
```bash
sudo apt-get install -y build-essential qt6-base-dev qt6-multimedia-dev \
  qt6-tools-dev qt6-l10n-tools libarchive-dev libwayland-dev wayland-protocols \
  libxcb-cursor0 pkg-config
```

**Fedora:**
```bash
sudo dnf install -y qt6-qtbase-devel qt6-qtmultimedia-devel qt6-linguist \
  libarchive-devel wayland-devel wayland-protocols-devel
```

### 编译 / Build

```bash
# Release 构建
CONFIG=release make -j$(nproc)

# Debug 构建
CONFIG=debug make -j$(nproc)
```

产物位于 `publish/Linux/<config>/`。

### 打包 AppImage

```bash
CONFIG=release make -j$(nproc)
make appimage
```

会在 `publish/Linux/release/` 下生成 `NeurolingsCE.AppImage`。

### 安装 / Install

```bash
make install PREFIX=/usr/local
```

安装内容：
- 二进制：`<PREFIX>/bin/shijima-qt`
- 库：`<PREFIX>/lib/libunarr.so.1`
- 桌面入口：`<PREFIX>/share/applications/`
- AppStream 元信息：`<PREFIX>/share/metainfo/`
- 图标：`<PREFIX>/share/icons/hicolor/512x512/apps/`

### 卸载 / Uninstall

```bash
make uninstall PREFIX=/usr/local
```

---

## macOS

### 安装依赖 / Install Dependencies

使用 MacPorts：

```bash
sudo port install qt6-qtbase qt6-qtmultimedia pkgconfig libarchive
```

### 编译 / Build

```bash
CONFIG=release make -j$(nproc)
```

### 打包 .app

```bash
make macapp
```

产物在 `publish/macOS/<config>/NeurolingsCE.app`。自动调用 `macdeployqt` 打包 Qt 框架。

---

## 构建产物 / Build Output

| 平台 | 输出路径 |
|------|---------|
| Windows (MinGW) | `publish/Windows/<config>/` |
| Linux | `publish/Linux/<config>/` |
| macOS | `publish/macOS/<config>/` |
| Windows (CMake/MSVC) | `build/` 或 `out/build/<config>/` (VS) |

## 清理 / Clean

```bash
# Make 构建
make clean

# CMake 构建
cmake --build build --target clean
# 或直接删除 build 目录
```

## 常见构建问题 / Common Build Issues

### Q: CMake 找不到 Qt6

确保设置了正确的 Qt 路径：
```bash
cmake -B build -DQt6_DIR=/path/to/Qt/6.8.x/gcc_64/lib/cmake/Qt6
```

### Q: MSVC 报 32-bit 错误

项目要求 x64 工具链。确保使用 "x64 Native Tools" 命令行或在 VS 中选择 x64 平台。

### Q: 缺少子模块

```bash
git submodule update --init --recursive
```

### Q: libshimejifinder 在 MSVC 上编译失败

MSVC 构建默认关闭 libshimejifinder（依赖 Unix 工具）。不影响基本功能，只是无法通过 libshimejifinder 导入部分格式的资源包（SimpleZipImporter 会作为替代）。
