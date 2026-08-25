# NeurolingsCE Manager

NeurolingsCE 的 Flutter 桌面管理器，提供桌宠、组合、商店、Codex 和运行时设置界面。

## 本地验证

```shell
flutter pub get
flutter analyze
flutter test
```

## 平台构建

以下命令必须在对应宿主系统执行：

```shell
flutter build windows --release
flutter build linux --release
flutter build macos --release
```

仓库根目录的 `.github/workflows/ci.yml` 会在三个原生宿主上执行相同门禁；
`.github/workflows/build-release.yaml` 负责将 Manager 与 Rust runtime、CLI 合并打包。
