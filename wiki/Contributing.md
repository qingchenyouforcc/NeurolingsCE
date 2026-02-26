# 贡献指南 / Contributing

感谢你对 NeurolingsCE 的关注！本页面介绍如何参与项目开发。

> Thank you for your interest in NeurolingsCE! This page explains how to contribute to the project.

## 开始之前 / Before You Start

### 了解项目结构

建议先阅读以下文档：
- [架构概览](Architecture) — 了解代码结构和核心类
- [构建指南](Building) — 确保你可以本地编译
- [平台抽象层](Platform-Abstraction) — 如果涉及平台相关代码

### 重要限制 / Important Limitations

- **libshijima 不接受外部贡献** — 上游 README 中明确声明。作为只读子模块使用。
- **libshimejifinder 和 cpp-httplib** — 同为只读子模块。
- 修改子模块代码请向对应的上游仓库提交。

## 开发环境 / Development Environment

### 推荐设置 / Recommended Setup

| 工具 | 推荐 |
|------|------|
| IDE | Visual Studio 2022 (Windows) / VS Code / CLion |
| C++ 标准 | C++17 |
| Qt 版本 | 6.8+ |
| 格式化 | 手动保持一致（无 .clang-format） |

### 获取代码 / Getting the Code

```bash
git clone https://github.com/qingchenyouforcc/NeurolingsCE.git
cd NeurolingsCE
git submodule update --init --recursive
```

### 本地构建验证 / Local Build Verification

提交 PR 前确保至少在一个平台上可以编译通过：

```bash
# Windows (MSVC)
cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug -DQt6_DIR=<your-qt-path>
cmake --build build

# Linux / macOS
CONFIG=debug make -j$(nproc)
```

## 代码规范 / Code Conventions

### 文件组织 / File Organization

| 规则 | 说明 |
|------|------|
| 头文件/源文件分离 | 头文件在 `include/shijima-qt/*.hpp`，实现在 `src/app/*.cc` |
| 文件扩展名 | 源文件用 `.cc`（不是 `.cpp`），头文件用 `.hpp` |
| 头文件保护 | 使用 `#pragma once` |
| License 头 | 每个源文件/头文件顶部包含 GPLv3 许可证样板 |

### 代码风格 / Code Style

项目没有 `.clang-format` 或 `.clang-tidy` 配置，请**匹配现有代码风格**：

| 方面 | 风格 |
|------|------|
| 缩进 | 4 空格 |
| 大括号 | K&R 风格（开括号不换行） |
| 命名 - 类 | PascalCase (`ShijimaManager`) |
| 命名 - 方法 | camelCase (`updateEnvironment`) |
| 命名 - 成员 | `m_` 前缀 (`m_mascots`) |
| 命名 - 常量/宏 | UPPER_SNAKE_CASE (`SHIJIMA_USE_QTMULTIMEDIA`) |
| Include 风格 | `#include "shijima-qt/Foo.hpp"` / `#include <shijima/...>` / `#include "Platform/..."` |

### 条件编译 / Conditional Compilation

```cpp
// 平台判断
#ifdef WIN32         // Windows
#ifdef __linux__     // Linux
#ifdef __APPLE__     // macOS

// 功能开关
#if SHIJIMA_USE_QTMULTIMEDIA
#if SHIJIMA_WITH_SHIMEJIFINDER
```

### 类型安全 / Type Safety

- **不要** 使用 C 风格强制转换
- **不要** 压制类型错误
- 使用 `static_cast<>`, `dynamic_cast<>` 等 C++ 转换
- 使用 `std::unique_ptr`, `std::shared_ptr` 管理资源（参考 `ShijimaWidget` 中的 `m_mascot`）

## 提交流程 / Submission Process

### 1. Fork 仓库

在 GitHub 上 fork [NeurolingsCE](https://github.com/qingchenyouforcc/NeurolingsCE)。

### 2. 创建分支

```bash
git checkout -b feature/my-feature
# 或
git checkout -b fix/my-bugfix
```

### 3. 开发与测试

- 确保代码可在至少一个平台上编译
- 如果修改了平台相关代码，尽量在相关平台上测试
- 如果修改了 UI，确保中英文翻译都正确

### 4. 提交代码

```bash
git add .
git commit -m "描述你的修改"
```

建议的提交信息格式：
- `feat: 添加 XXX 功能` — 新功能
- `fix: 修复 XXX 问题` — Bug 修复
- `refactor: 重构 XXX` — 重构
- `docs: 更新 XXX 文档` — 文档
- `i18n: 更新 XXX 翻译` — 翻译

### 5. 创建 Pull Request

推送到 fork 后，在 GitHub 上创建 PR 到 `main` 分支。

PR 中请说明：
- 修改了什么
- 为什么要修改
- 如何测试

### 6. CI 检查

PR 会触发 CI 构建（`build-debug.yaml`），在 Windows、Linux (x86_64 + arm64)、macOS 上编译验证。

## 可贡献的方向 / Areas for Contribution

### 已知 TODO（来自 README）
- 修复 Taskbar bug
- i18n 支持扩展

### 一般方向
- **Bug 修复** — 查看 [Issues](https://github.com/qingchenyouforcc/NeurolingsCE/issues)
- **新语言翻译** — 参见 [国际化指南](Internationalization)
- **文档改进** — 修正错误、添加示例
- **平台支持** — 改善 Linux 桌面环境兼容性
- **性能优化** — 精灵图渲染、内存使用

## 联系方式 / Contact

- **GitHub Issues**: [问题反馈](https://github.com/qingchenyouforcc/NeurolingsCE/issues)
- **反馈 QQ 群**: 423902950
- **交流 QQ 群**: 125081756

如果你对 Neuro 社区项目开发感兴趣，可以联系作者加入 **NeuForge Center**。

## 相关页面 / Related Pages

- [架构概览](Architecture)
- [构建指南](Building)
- [国际化](Internationalization)
- [CI/CD](CI-CD)
