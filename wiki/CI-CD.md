# CI/CD

本页面介绍 NeurolingsCE 的持续集成和发布流程。

> This page describes the CI/CD (Continuous Integration / Continuous Delivery) pipeline of NeurolingsCE.

## 概览 / Overview

NeurolingsCE 使用 GitHub Actions 进行 CI/CD，包含两个工作流：

| 工作流 | 触发条件 | 目的 |
|--------|---------|------|
| `build-debug.yaml` | Push 到 `main` / PR 打开/同步 | 编译验证，确保代码可构建 |
| `build-release.yaml` | 手动触发 (`workflow_dispatch`) | 构建发布版本，生成可分发产物 |

## 构建矩阵 / Build Matrix

两个工作流都覆盖三大平台：

| 平台 | Runner | 架构 | 构建方式 |
|------|--------|------|---------|
| Windows | `ubuntu-latest` (Docker) | x86_64 | MinGW 交叉编译 |
| Linux | `ubuntu-22.04` | x86_64 | 原生 Make |
| Linux | `ubuntu-24.04-arm` | arm64 | 原生 Make |
| macOS | `macos-14` | arm64 (Apple Silicon) | 原生 Make |

> **注意 / Note**: Windows 构建在 Linux runner 上通过 Docker 交叉编译，而非在 Windows runner 上进行。这使用的是 Fedora-based 的 `shijima-qt-dev` Docker 镜像。

## Debug 工作流 / Debug Workflow

**文件**: `.github/workflows/build-debug.yaml`

**触发条件 / Triggers:**
- Push 到 `main` 分支
- PR 打开、同步或重新打开

**流程 / Steps:**

### Windows (Docker 交叉编译)
1. Checkout 代码（含递归子模块）
2. 获取完整的 git 历史（libshijima 需要 commit count）
3. 缓存/构建 Docker 镜像 (`docker-fedora-42-qt6-mingw`)
4. 在 Docker 中执行 `mingw64-make -j$(nproc)` (debug)
5. 上传 `shijima-qt.exe` 作为 artifact（保留 1 天）

### Linux (x86_64 + arm64)
1. Checkout 代码
2. 安装 Qt 6.8.2（通过 `jdpurcell/install-qt-action`，带缓存）
3. 安装系统依赖（`libxcb-cursor0`、`libarchive-dev`、`wayland-*`）
4. `make -j$(nproc)` (debug)
5. 上传 `shijima-qt` + `libunarr.so.1` 作为 artifact

### macOS (arm64)
1. Checkout 代码
2. 缓存 MacPorts Qt6（`/opt/local`），首次安装 MacPorts + Qt6
3. `make -j6` (debug)
4. 上传 `shijima-qt` 作为 artifact

## Release 工作流 / Release Workflow

**文件**: `.github/workflows/build-release.yaml`

**触发条件 / Trigger:** 手动触发 (`workflow_dispatch`)

与 Debug 工作流的区别：

| 方面 | Debug | Release |
|------|-------|---------|
| `CONFIG` | `debug` | `release` |
| 额外步骤 | 无 | Linux: `make appimage`; macOS: `make macapp` |
| 许可证 | 不包含 | 复制 `licenses/` 目录到产物中 |
| 产物范围 | 仅二进制 | 完整发布包（含 AppImage、.app 等） |
| Artifact 保留 | 1 天 | 1 天 |

### Release 产物 / Release Artifacts

| 名称 | 内容 |
|------|------|
| `release-windows-x86_64` | `publish/Windows/release/` 完整目录 |
| `release-linux-x86_64` | `publish/Linux/release/`（含 AppImage） |
| `release-linux-arm64` | `publish/Linux/release/`（含 AppImage） |
| `release-macos-arm64` | `publish/macOS/release/`（含 .app） |

## 缓存策略 / Caching Strategy

| 缓存项 | Key | 说明 |
|--------|-----|------|
| Docker 镜像 | `docker-fedora-42-qt6-mingw` | 保存为 `.tar.gz`，避免重复构建 |
| Qt6 (Linux) | 由 `install-qt-action` 管理 | 自动缓存 Qt 安装 |
| MacPorts Qt6 | `macos-14-macports-qt6` | 缓存整个 `/opt/local` 目录 |

## 子模块处理 / Submodule Handling

CI 中需要特别处理子模块的 git 历史：

```yaml
# 递归克隆子模块
- uses: actions/checkout@v4
  with:
    submodules: 'recursive'

# libshijima 需要完整历史（可能用于 commit count 或版本号）
- run: |
    git fetch --unshallow origin $BRANCH_NAME
    pushd libshijima
    git fetch --unshallow origin main
    popd
```

## macOS 特殊处理 / macOS Special Handling

macOS 构建中有一个特殊的 `gtar` 权限处理：

```yaml
# 缓存恢复需要 root 权限的 gtar
- name: Temporarily let gtar run as root
  run: |
    sudo chown 0:0 /opt/homebrew/bin/gtar
    sudo chmod u+s /opt/homebrew/bin/gtar

# 缓存操作后立即撤销
- name: Revoke gtar's root permissions
  run: |
    sudo chmod u-s /opt/homebrew/bin/gtar
```

这是因为 MacPorts 安装到 `/opt/local`，GitHub Actions 的缓存恢复需要 root 权限来解压到该路径。

## 本地复现 CI / Reproducing CI Locally

### Windows (Docker)

```bash
docker build -t shijima-qt-dev dev-docker
docker run -e CONFIG=debug --rm -v "$(pwd)":/work \
  shijima-qt-dev bash -c 'mingw64-make clean && mingw64-make -j$(nproc)'
```

### Linux

```bash
# 安装依赖后
CONFIG=debug make clean && make -j$(nproc)
```

### macOS

```bash
# 安装 MacPorts + Qt6 后
CONFIG=debug make clean && make -j6
```

## 相关页面 / Related Pages

- [构建指南](Building) — 本地构建详解
- [贡献指南](Contributing) — PR 与 CI 检查
