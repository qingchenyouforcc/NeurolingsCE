# 国际化 / Internationalization (i18n)

本页面介绍 NeurolingsCE 的多语言翻译机制和如何添加新语言。

> This page explains the i18n mechanism in NeurolingsCE and how to add new languages.

## 概览 / Overview

NeurolingsCE 使用 Qt 的标准国际化框架：

| 组件 | 说明 |
|------|------|
| `.ts` 文件 | 翻译源文件（XML 格式），手工维护 |
| `.qm` 文件 | 编译后的二进制翻译文件 |
| `lrelease` | Qt 工具，将 `.ts` 编译为 `.qm` |
| `lupdate` | Qt 工具，从源码提取可翻译字符串到 `.ts`（项目中不自动运行） |
| `QTranslator` | Qt 类，运行时加载翻译 |

## 当前支持的语言 / Supported Languages

| 语言 | 文件 | 状态 |
|------|------|------|
| English | 默认（源码中的字符串） | 完整 |
| 简体中文 | `translations/shijima-qt_zh_CN.ts` | 完整 |

## 文件结构 / File Structure

```
translations/
├── i18n.qrc              # Qt 资源文件，定义翻译资源
└── shijima-qt_zh_CN.ts   # 简体中文翻译源文件
```

## 构建过程 / Build Process

### CMake

```cmake
# 只编译 .ts → .qm，不自动运行 lupdate
qt_add_lrelease(
    TS_FILES translations/shijima-qt_zh_CN.ts
    QM_FILES_OUTPUT_VARIABLE QM_FILES
)

# 嵌入到 Qt 资源系统
qt_add_resources(NeurolingsCE "translations"
    PREFIX "/i18n"
    BASE "${CMAKE_CURRENT_BINARY_DIR}"
    FILES ${QM_FILES}
)
```

> **设计决策 / Design Decision**: 项目**不自动运行 `lupdate`**，因为那会覆盖手工维护的 `.ts` 文件中未完成的翻译条目。翻译文件由开发者手动维护。

### Make

```makefile
translations/%.qm: translations/%.ts
	$(LRELEASE) $< -qm $@

qrc_i18n.cc: translations/i18n.qrc $(QM_FILES)
	$(RCC) -o $@ $<
```

## 运行时语言切换 / Runtime Language Switching

`ShijimaManager` 中管理翻译器实例：

```cpp
// 成员变量
QTranslator *m_translator;      // 应用翻译
QTranslator *m_qtTranslator;    // Qt 内置控件翻译
QString m_currentLanguage;       // 当前语言代码

// 切换语言
void switchLanguage(const QString &langCode);

// 重新翻译 UI
void retranslateUi();
```

语言切换流程：
1. 移除旧翻译器
2. 加载新的 `.qm` 文件（从嵌入的 Qt 资源中）
3. 安装到 `QApplication`
4. 调用 `retranslateUi()` 更新所有 UI 文本

## 添加新语言 / Adding a New Language

### 步骤 1：创建翻译文件

1. 复制现有的 `.ts` 文件作为模板：
   ```bash
   cp translations/shijima-qt_zh_CN.ts translations/shijima-qt_ja_JP.ts
   ```

2. 编辑新文件，修改 `<TS>` 标签的 `language` 属性：
   ```xml
   <TS version="2.1" language="ja_JP">
   ```

3. 翻译所有 `<source>` 对应的 `<translation>` 内容

### 步骤 2：注册翻译文件

**CMake** — 在 `CMakeLists.txt` 的 `qt_add_lrelease` 中添加：
```cmake
qt_add_lrelease(
    TS_FILES
        translations/shijima-qt_zh_CN.ts
        translations/shijima-qt_ja_JP.ts    # 新增
    QM_FILES_OUTPUT_VARIABLE QM_FILES
)
```

**Make** — 在 `Makefile` 中更新 `TS_FILES`：
```makefile
TS_FILES = translations/shijima-qt_zh_CN.ts \
           translations/shijima-qt_ja_JP.ts
```

### 步骤 3：更新 Qt 资源文件

编辑 `translations/i18n.qrc`，添加新的 `.qm` 文件引用。

### 步骤 4：更新语言切换 UI

在 `ShijimaManager` 的语言切换逻辑中添加新语言选项。

## 翻译指南 / Translation Guidelines

1. **保持一致性** — 同一术语在不同位置使用相同的翻译
2. **保留占位符** — 如 `%1`、`%2` 等 Qt 格式化占位符必须保留
3. **适应 UI 空间** — 翻译后的文本长度不应显著超过原文
4. **使用 Qt Linguist** — 推荐使用 Qt Linguist 工具编辑 `.ts` 文件，它提供上下文预览

## 可翻译字符串来源 / Translatable String Sources

在源码中使用以下方式标记可翻译字符串：

```cpp
// 在 QObject 子类中
tr("Hello World")

// 在非 QObject 上下文中
QCoreApplication::translate("main", "Shijima-Qt failed to start.")
```

## 相关页面 / Related Pages

- [构建指南](Building) — Qt LinguistTools 的配置
- [贡献指南](Contributing) — 如何提交翻译
