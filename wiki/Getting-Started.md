# 快速开始 / Getting Started

本页面帮助你快速上手使用 NeurolingsCE。

> This page helps you get started with NeurolingsCE quickly.

## 下载安装 / Download & Install

### 下载 / Download

前往 [Releases 页面](https://github.com/qingchenyouforcc/NeurolingsCE/releases/latest) 下载对应平台的版本：

| 平台 | 文件格式 | 架构 |
|------|---------|------|
| Windows | `.exe` + DLL 文件夹 | x86_64 |
| Linux | `.AppImage` 或二进制 + `libunarr.so.1` | x86_64 / arm64 |
| macOS | `.app` 应用包 | arm64 |

### Windows

1. 下载 `release-windows-x86_64` 压缩包
2. 解压到任意目录
3. 双击 `NeurolingsCE.exe`（或旧名 `shijima-qt.exe`）运行

> **注意 / Note**: 仅支持 64 位 Windows。已在 Windows 11 上测试，Windows 10 应该也兼容。

### Linux

**AppImage（推荐）：**

```bash
chmod +x NeurolingsCE.AppImage
./NeurolingsCE.AppImage
```

**手动安装：**

```bash
# 将二进制文件放到系统路径
sudo install -Dm755 shijima-qt /usr/local/bin/shijima-qt
sudo install -Dm755 libunarr.so.1 /usr/local/lib/libunarr.so.1
```

**桌面环境支持 / Desktop Environment Support：**
- **KDE Plasma 6** — 窗口追踪开箱即用，对用户透明
- **GNOME 46** — 首次运行后需要**重新登录**以激活 Shell 扩展
- **其他 DE** — 窗口追踪功能不可用，但看板娘仍可在桌面上运行

### macOS

1. 下载 `NeurolingsCE.app`
2. 拖入 `/Applications` 目录
3. 首次运行时需在 **系统设置 → 隐私与安全 → 辅助功能** 中授予权限
4. 最低系统版本：macOS 13 (Ventura)

## 首次使用 / First Run

### 启动 / Launching

启动后，NeurolingsCE 会：

1. 检查是否已有实例在运行（通过 HTTP ping `127.0.0.1:32456`）
2. 如果没有，显示管理器主窗口和一只默认看板娘

> **单实例机制 / Single Instance**: 同一时间只能运行一个 NeurolingsCE 实例。如果已有实例运行，新启动的进程会提示并退出。

### 管理器窗口 / Manager Window

管理器窗口是主控制界面，包含：

- **看板娘列表** — 显示已加载的看板娘资源包
- **工具栏** — 生成、导入、删除看板娘等操作
- **窗口模式切换** — 在桌面模式和沙盒窗口模式间切换

### 基本操作 / Basic Operations

| 操作 | 方法 |
|------|------|
| 生成看板娘 | 在列表中选择看板娘，点击 "Spawn" 按钮，或双击列表项 |
| 拖拽看板娘 | 鼠标左键按住看板娘拖动 |
| 右键菜单 | 鼠标右键点击看板娘 |
| 导入资源包 | 拖拽 `.zip` 到管理器窗口，或点击 "Import" 按钮 |
| 关闭所有看板娘 | 右键菜单 → "Dismiss All" |
| 退出程序 | 管理器窗口 → 关闭 / 退出 |

### 窗口模式 / Window Mode

NeurolingsCE 支持两种运行模式：

- **桌面模式**（默认）— 看板娘在桌面上自由活动，可与其他窗口互动
- **沙盒窗口模式** — 看板娘在独立窗口内运行，不影响其他窗口

可在管理器工具栏中切换。

### CLI 模式 / CLI Mode

当传入命令行参数时，NeurolingsCE 进入 CLI 模式，通过 HTTP API 与正在运行的实例通信：

```bash
# 调用正在运行实例的 API
./NeurolingsCE <command> [args...]
```

## 下一步 / Next Steps

- [导入看板娘资源包](Mascot-Packs)
- [通过 HTTP API 控制看板娘](HTTP-API)
- [从源码构建](Building)
