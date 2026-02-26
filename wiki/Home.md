# NeurolingsCE Wiki

**NeurolingsCE** 是一个跨平台桌面看板娘（Shimeji）模拟应用，基于 [Shijima-Qt](https://github.com/pixelomer/Shijima-Qt) 深度修改而来。使用 C++17 / Qt6 构建，支持 Windows、Linux 和 macOS。

> **NeurolingsCE** is a cross-platform desktop mascot (Shimeji) simulator, extensively modified from [Shijima-Qt](https://github.com/pixelomer/Shijima-Qt). Built with C++17 / Qt6, supporting Windows, Linux, and macOS.

---

## 功能特性 / Features

| 功能 | 说明 |
|------|------|
| 跨平台 | Windows (x64) / Linux (x86_64, arm64) / macOS (arm64) |
| 看板娘兼容 | 兼容 Shimeji-ee 格式资源包 |
| 拖放导入 | 直接拖拽 `.zip` 看板娘压缩包到窗口即可导入 |
| 窗口模式 | 可在独立沙盒窗口中运行看板娘 |
| 鼠标交互 | 拖拽移动、右键菜单操作 |
| HTTP REST API | 内置 API 接口，可通过外部程序控制看板娘 |
| 多语言 | 支持英文和简体中文 |
| 音效 | 可选的 Qt Multimedia 音效支持 |
| 多显示器 | 支持多屏幕环境 |
| 自定义缩放 | 可调整看板娘的显示大小 |

## 快速导航 / Quick Navigation

### 用户指南 / User Guide
- **[快速开始 / Getting Started](Getting-Started)** — 下载、安装与首次使用
- **[看板娘资源包 / Mascot Packs](Mascot-Packs)** — 如何导入和管理看板娘
- **[HTTP API](HTTP-API)** — 通过 REST API 控制看板娘
- **[常见问题 / FAQ](FAQ)** — 常见问题解答

### 开发者指南 / Developer Guide
- **[构建指南 / Building](Building)** — 从源码编译项目
- **[架构概览 / Architecture](Architecture)** — 项目整体架构与核心类
- **[平台抽象层 / Platform Abstraction](Platform-Abstraction)** — 跨平台实现细节
- **[国际化 / Internationalization](Internationalization)** — 多语言翻译指南
- **[CI/CD](CI-CD)** — 持续集成与发布流程
- **[贡献指南 / Contributing](Contributing)** — 如何参与项目开发

## 项目信息 / Project Info

| 项目 | 信息 |
|------|------|
| 仓库 | [github.com/qingchenyouforcc/NeurolingsCE](https://github.com/qingchenyouforcc/NeurolingsCE) |
| 许可证 | [GNU General Public License v3.0](https://github.com/qingchenyouforcc/NeurolingsCE/blob/main/LICENSE) |
| 作者 | [轻尘呦](https://space.bilibili.com/178381315) |
| 上游项目 | [pixelomer/Shijima-Qt](https://github.com/pixelomer/Shijima-Qt) |
| 反馈 QQ 群 | 423902950 |
| 交流 QQ 群 | 125081756 |

## 核心依赖 / Core Dependencies

| 依赖 | 用途 |
|------|------|
| [libshijima](https://github.com/pixelomer/libshijima) | 看板娘模拟引擎（XML 解析、行为逻辑、duktape 脚本） |
| [libshimejifinder](https://github.com/pixelomer/libshimejifinder) | 看板娘资源包解压导入 |
| [cpp-httplib](https://github.com/yhirose/cpp-httplib) | HTTP 服务端/客户端（header-only） |
| [Qt 6](https://www.qt.io/) | GUI 框架 |
| [miniz](https://github.com/richgel999/miniz) | 轻量级 ZIP 解压库（用于 SimpleZipImporter） |
