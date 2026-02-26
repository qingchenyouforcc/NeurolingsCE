# 看板娘资源包 / Mascot Packs

本页面介绍如何导入、管理和创建看板娘资源包。

> This page explains how to import, manage, and create mascot packs.

## 兼容格式 / Compatible Formats

NeurolingsCE 兼容 **Shimeji-ee** 格式的看板娘资源包。

### 资源包结构 / Pack Structure

一个标准的看板娘资源包包含：

```
<MascotName>/
├── conf/
│   ├── actions.xml      # 动作定义
│   └── behaviors.xml    # 行为定义
├── img/
│   ├── shime1.png       # 精灵图帧 1
│   ├── shime2.png       # 精灵图帧 2
│   ├── ...
│   └── shime46.png      # 精灵图帧 46
└── sound/               # (可选) 音效文件
    └── *.wav
```

### 支持的 ZIP 布局 / Supported ZIP Layouts

通过 ZIP 导入时，NeurolingsCE 能自动识别以下几种布局：

| 布局类型 | 结构 | 说明 |
|---------|------|------|
| **根级 / Root Level** | `actions.xml` + `behaviors.xml` + `img/` 直接在 ZIP 根目录 | 最简单的格式 |
| **Shimeji-ee** | `shimeji-ee.jar` + `conf/` + `img/<name>/` | 标准 Shimeji-ee 包 |
| **子目录 / Subdirectory** | `<name>/conf/actions.xml` + `img/` | 包含子目录的包 |
| **纯图片 / Bare Images** | 只有 `shime1.png` ~ `shime46.png` | 使用内置默认 XML |

## 导入看板娘 / Importing Mascots

### 方式一：拖放导入（推荐）

1. 准备好看板娘 `.zip` 文件
2. 直接将文件拖放到管理器窗口上
3. 程序会自动识别格式、解压并加载

### 方式二：点击导入

1. 在管理器窗口中点击 "Import" 按钮
2. 选择 `.zip` 文件
3. 等待导入完成

### 方式三：手动导入

1. 找到看板娘存储目录（通常在应用数据目录下的 `mascots/` 子目录）
2. 将看板娘目录直接放入
3. 重启 NeurolingsCE 或触发重新加载

### 导入技术细节 / Import Technical Details

NeurolingsCE 使用两种导入器：

| 导入器 | 条件 | 能力 |
|--------|------|------|
| **libshimejifinder** | Linux/macOS 构建，或 MSVC 手动启用 | 支持 `.zip`、`.rar`、`.7z` 等多种归档格式 |
| **SimpleZipImporter** | MSVC 构建默认使用 | 仅支持 `.zip`，基于 miniz 库 |

> **MSVC 用户注意 / MSVC Users Note**: MSVC 构建默认不包含 libshimejifinder（因依赖 Unix 工具），使用内置的 SimpleZipImporter 作为替代。功能略有限制但足以覆盖大部分场景。

## 管理看板娘 / Managing Mascots

### 生成 / Spawning

- 在列表中选择看板娘，点击 "Spawn" 按钮
- 或双击列表项
- 可同时生成多个相同或不同的看板娘实例

### 删除 / Deleting

- 在列表中选择看板娘
- 点击 "Delete" 按钮
- **默认看板娘不可删除** — 它是内嵌在程序中的

### 重新加载 / Reloading

修改了看板娘资源文件后，可通过管理器重新加载：
- 会卸载旧的缓存资源
- 重新解析 XML 配置
- 重新加载精灵图

## 默认看板娘 / Default Mascot

NeurolingsCE 内置了一个默认看板娘，在编译时通过 `cmake/BundleDefaultMascot.cmake` 嵌入到二进制中。

默认看板娘包含：
- `src/assets/DefaultMascot/behaviors.xml`
- `src/assets/DefaultMascot/actions.xml`
- `src/assets/DefaultMascot/img/shime1.png` ~ `shime46.png`（共 46 帧）

> **不可禁用 / Cannot Disable**: `SHIJIMA_WITH_DEFAULT_MASCOT=OFF` 当前不受支持。默认看板娘始终会被编译到程序中。

## XML 配置说明 / XML Configuration

### actions.xml（动作定义）

定义看板娘的具体动作（Animation、Move、Stay 等）：

```xml
<ActionList>
    <Action Name="Fall" Type="Fall" ...>
        <Animation>
            <Pose Image="/shime1.png" Duration="5" />
            <Pose Image="/shime2.png" Duration="5" />
        </Animation>
    </Action>
    <!-- 更多动作... -->
</ActionList>
```

### behaviors.xml（行为定义）

定义看板娘的行为逻辑（什么条件下执行什么动作）：

```xml
<BehaviorList>
    <Behavior Name="Fall" Frequency="1" Condition="...">
        <NextBehaviorList>
            <BehaviorReference Name="Landing" Frequency="1" />
        </NextBehaviorList>
    </Behavior>
    <!-- 更多行为... -->
</BehaviorList>
```

> **注意 / Note**: XML 格式由 libshijima 解析，遵循 Shimeji-ee 的规范。具体的 XML Schema 请参考 Shimeji-ee 的相关文档。

## 精灵图规范 / Sprite Specifications

| 属性 | 要求 |
|------|------|
| 格式 | PNG（带 alpha 通道） |
| 命名 | `shime<N>.png`（N = 1, 2, 3, ...） |
| 大小 | 无严格限制，但所有帧应保持一致 |
| 透明度 | 透明区域在 Linux 上用于生成窗口遮罩 |

## 音效 / Sound Effects

如果构建时启用了 `SHIJIMA_USE_QTMULTIMEDIA`，看板娘可以播放音效。

- 音效文件放在资源包的 `sound/` 目录下
- 支持 Qt Multimedia 支持的音频格式（WAV 等）
- 在 XML 配置中引用音效文件名

## 资源来源 / Where to Find Mascots

常见的 Shimeji 资源来源：
- [DeviantArt](https://www.deviantart.com/) — 搜索 "shimeji"
- [Shimeji-ee 官方](https://kilkakon.com/shimeji/) — Shimeji-ee 主页
- 各种二次创作社区

> **重要 / Important**: 请尊重创作者的版权和使用条款。

## 相关页面 / Related Pages

- [快速开始](Getting-Started) — 首次导入看板娘
- [架构概览](Architecture) — MascotData 和 SimpleZipImporter 的设计
- [HTTP API](HTTP-API) — 通过 API 生成和管理看板娘
