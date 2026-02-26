# NeurolingsCE

**English | [中文](README.md)**

A cross-platform desktop mascot (Shimeji) application, extensively modified from [Shijima-Qt](https://github.com/pixelomer/Shijima-Qt).

Built with C++17 / Qt6, supporting Windows, Linux, and macOS.

## Features

- 🖥️ Cross-platform support (Windows / Linux / macOS)
- 🎭 Compatible with Shimeji-ee format mascot packs
- 📦 Drag-and-drop mascot pack import
- 🪟 Window mode — run mascots in standalone sandbox windows
- 🖱️ Mouse interaction — drag, right-click menu
- 📡 HTTP REST API (`localhost:32456`)
- 🌐 Multi-language support (English / Simplified Chinese)
- 🔊 Optional sound effects (Qt Multimedia)
- 🖥️ Multi-monitor support
- 📐 Custom scaling

## Download

- [Latest Release](https://github.com/qingchenyouforcc/NeurolingsCE/releases/latest)
- [All Releases](https://github.com/qingchenyouforcc/NeurolingsCE/releases)

## Documentation

📖 **[Wiki](https://github.com/qingchenyouforcc/NeurolingsCE/wiki)** — Full documentation including getting started, build guide, architecture, HTTP API, FAQ, and more.

## Building

### Prerequisites

- C++17 compiler (MSVC 2022 / GCC / Clang)
- Qt 6.8+ (Core, Gui, Widgets, Concurrent, LinguistTools)
- CMake 3.21+ (Windows/MSVC) or Make (Linux/macOS)

Initialize submodules first:

```bash
git submodule update --init --recursive
```

### Windows (MSVC + CMake)

```bash
cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DQt6_DIR=D:/Qt/6.8.3/msvc2022_64/lib/cmake/Qt6
cmake --build build
```

You can also open the project directly in Visual Studio — `CMakeSettings.json` includes pre-configured `x64-Debug` and `x64-Release` profiles.

### Windows (MinGW Cross-Compilation via Docker)

```bash
docker build -t neurolingsce-dev dev-docker
docker run -e CONFIG=release --rm -v "$(pwd)":/work neurolingsce-dev bash -c 'mingw64-make -j$(nproc)'
```

### Linux

After installing Qt6 development dependencies:

```bash
CONFIG=release make -j$(nproc)
```

### macOS

1. Install dependencies via MacPorts:

```bash
sudo port install qt6-qtbase qt6-qtmultimedia pkgconfig libarchive
```

2. Build:

```bash
CONFIG=release make -j$(nproc)
```

## Platform Notes

### Windows

Only x64 toolchain is supported. Tested on Windows 11; Windows 10 should also work. Window tracking works out of the box.

### Linux

Supports KDE Plasma 6 and GNOME 46 (Wayland / X11). On first run, a shell plugin is automatically installed to obtain foreground window information:
- **KDE** — Transparent to the user, no action needed.
- **GNOME** — Requires re-login after first run to restart the Shell. The application will prompt accordingly.
- **Other desktop environments** — Window tracking is not available.

### macOS

Requires Accessibility permission to obtain foreground window information. Minimum system version: macOS 13.

## HTTP API

A built-in HTTP REST API runs at `http://127.0.0.1:32456`, allowing external programs to control mascots.

See detailed documentation at [src/docs/HTTP-API.md](src/docs/HTTP-API.md).

## Project Structure

```
NeurolingsCE/
├── src/app/              # Qt application layer
├── src/platform/Platform/ # Platform abstraction (Windows/Linux/macOS)
├── include/shijima-qt/   # Public headers
├── libshijima/           # [submodule] Core mascot simulation engine
├── libshimejifinder/     # [submodule] Mascot pack import/extraction
├── cpp-httplib/          # [submodule] HTTP server (header-only)
├── translations/         # i18n translation files
├── cmake/                # CMake helper scripts
├── src/assets/           # Bundled default mascot assets
└── src/packaging/        # Desktop entry, icons, AppStream metadata
```

## Credits

This project is based on [Shijima-Qt](https://github.com/pixelomer/Shijima-Qt) by [pixelomer](https://github.com/pixelomer), with extensive modifications and feature enhancements.

Core dependencies:
- [libshijima](https://github.com/pixelomer/libshijima) — Mascot simulation engine
- [libshimejifinder](https://github.com/pixelomer/libshimejifinder) — Mascot pack parser
- [cpp-httplib](https://github.com/yhirose/cpp-httplib) — HTTP library
- [Qt 6](https://www.qt.io/) — GUI framework

## Contact

- **Author**: [轻尘呦](https://space.bilibili.com/178381315)
- **Repository**: https://github.com/qingchenyouforcc/NeurolingsCE
- **Bug Reports**: [GitHub Issues](https://github.com/qingchenyouforcc/NeurolingsCE/issues)
- **Feedback QQ Group**: 423902950
- **Chat QQ Group**: 125081756

**Interested in Neuro community project development?**

**Contact me to join NeuForge Center**

**Join STNC to learn more**

**STNC Swarm Tech Intelligence Center QQ Group: 125081756**

**STNC Project Feedback QQ Group: 423902950**

## License

This project is open-sourced under the [GNU General Public License v3.0](LICENSE).

The upstream Shijima-Qt README is available at [Shijima-Qt_README.md](Shijima-Qt_README.md).

![](https://qingchenyou-1301914189.cos.ap-beijing.myqcloud.com/681dcdd42da7fc5484c1dd3a9875b54a_324.png)

---
## Ads

If you're interested in Neuro fan fiction, please join the literary society~

**NeuroEcho Literary Society QQ Group: 1063898428**

Please join the NeuroSama Bar group~

**NeuroSama Bar QQ Group: 734688012**

---

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=qingchenyouforcc/NeurolingsCE&type=Date)](https://star-history.com/#qingchenyouforcc/NeurolingsCE&Date)
