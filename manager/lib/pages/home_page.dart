import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:file_picker/file_picker.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter/services.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../api/runtime_api.dart';
import '../state/app_state.dart';

/// 主页：桌宠库（对齐原版 ManagerHomePage）。
/// 动作条 Spawn Random / Import / Refresh / Show Folder；
/// 左侧库列表（64px 预览、tooltip、Enter/双击召唤、多选），
/// 右侧 Details 面板（96×96 预览 + 元数据 + Delete Selected）。
class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  /// 选中的模板名集合（多选，ExtendedSelection 语义）。
  final Set<String> _selected = {};

  /// 模板名 → 预览 PNG（不能按不稳定的 id 缓存，导入后下标会变）。
  final Map<String, Uint8ListAware> _previews = {};

  bool _isSelected(String name) => _selected.contains(name);

  void _toggle(String name, bool selected) {
    setState(() {
      if (selected) {
        _selected.add(name);
      } else {
        _selected.remove(name);
      }
    });
  }

  /// 单击选择：默认单选，Ctrl/Meta 追加或取消。
  void _select(String name) {
    final additive =
        HardwareKeyboard.instance.isControlPressed ||
        HardwareKeyboard.instance.isMetaPressed;
    setState(() {
      if (additive) {
        if (_selected.contains(name)) {
          _selected.remove(name);
        } else {
          _selected.add(name);
        }
      } else {
        _selected
          ..clear()
          ..add(name);
      }
    });
  }

  Future<void> _spawnRandom() async {
    final state = context.read<AppState>();
    if (state.templates.isEmpty) return;
    final pick = state.templates[Random().nextInt(state.templates.length)];
    await state.spawn(pick.name);
  }

  Future<void> _import() async {
    final state = context.read<AppState>();
    final l10n = AppLocalizations.of(context);
    final picked = await FilePicker.platform.pickFiles(
      type: FileType.any,
      allowMultiple: true,
      dialogTitle: l10n.importButton,
    );
    if (picked == null) return;
    final files = picked.files.map((f) => f.path).whereType<String>().toList();
    if (files.isEmpty) return;
    for (final path in files) {
      final output = await state.importArchive(path);
      if (!mounted) return;
      displayInfoBar(
        context,
        builder: (ctx, close) {
          return InfoBar(
            title: Text(
              output.startsWith('{') || output.contains('imported')
                  ? l10n.homeImportDone
                  : l10n.error,
            ),
            content: Text(output),
            severity: output.contains('imported') || output.startsWith('{')
                ? InfoBarSeverity.success
                : InfoBarSeverity.error,
            action: IconButton(
              icon: const Icon(FluentIcons.clear),
              onPressed: close,
            ),
          );
        },
      );
    }
  }

  Future<void> _showFolder() async {
    try {
      final result = await context.read<AppState>().api.command({
        'command': 'storage_path',
      });
      final path = result['path'] as String?;
      if (path == null || path.isEmpty) return;
      if (Platform.isWindows) {
        await Process.run('explorer', [path]);
      } else if (Platform.isMacOS) {
        await Process.run('open', [path]);
      } else {
        await Process.run('xdg-open', [path]);
      }
    } catch (_) {
      // 运行时离线时忽略。
    }
  }

  Future<void> _spawnByName(String name) async {
    await context.read<AppState>().spawn(name);
  }

  /// 拉取模板预览图（runtime preview_png，按名称缓存）。
  Future<void> _loadPreview(String name) async {
    if (_previews.containsKey(name)) return;
    try {
      final result = await context.read<AppState>().api.command({
        'command': 'preview_png',
        'name': name,
      });
      final base64Text = result['preview_base64'] as String?;
      if (base64Text != null) {
        final bytes = base64Decode(base64Text);
        if (mounted) {
          setState(() => _previews[name] = Uint8ListAware(bytes));
        }
      }
    } catch (_) {
      // 预览缺失时保持占位。
    }
  }

  Future<void> _deleteSelected() async {
    final l10n = AppLocalizations.of(context);
    final state = context.read<AppState>();
    // 内置默认模板不可删除（对齐原版 deletable()==false）。
    final deletable = _selected
        .where((name) => name != '@' && name != 'Default')
        .toList();
    if (deletable.isEmpty) return;
    final shown = deletable.take(5).join(', ');
    final more = deletable.length > 5
        ? '\n${l10n.homeAndMore(deletable.length - 5)}'
        : '';
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => ContentDialog(
        title: Text(l10n.deleteSelected),
        content: Text(
          '${l10n.homeDeleteConfirm(deletable.length)}\n\n$shown$more',
        ),
        actions: [
          Button(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: Text(l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: Text(l10n.delete),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    for (final name in deletable) {
      try {
        await state.api.command({
          'command': 'remove_mascot_template',
          'mascot_name': name,
        });
      } catch (e) {
        if (!mounted) return;
        displayInfoBar(
          context,
          builder: (ctx, close) {
            return InfoBar(
              title: Text(l10n.error),
              content: Text('$name: $e'),
              severity: InfoBarSeverity.error,
            );
          },
        );
      }
    }
    setState(() => _selected.clear());
    await state.refresh();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final state = context.watch<AppState>();
    final templates = state.templates;
    final selectedTemplate = _selected.length == 1
        ? templates.where((t) => t.name == _selected.first).firstOrNull
        : null;

    final hasImported = templates.any(
      (t) => t.name != '@' && t.name != 'Default',
    );
    return ScaffoldPage.scrollable(
      header: PageHeader(
        title: Text(l10n.homeTitle),
        commandBar: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              FilledButton(
                onPressed: templates.isEmpty ? null : _spawnRandom,
                child: Text(l10n.spawnRandom),
              ),
              const SizedBox(width: 8),
              Button(onPressed: _import, child: Text(l10n.importButton)),
              const SizedBox(width: 8),
              Button(
                onPressed: () => state.refresh(reloadLibrary: true),
                child: Text(l10n.refresh),
              ),
              const SizedBox(width: 8),
              Button(onPressed: _showFolder, child: Text(l10n.showFolder)),
            ],
          ),
        ),
      ),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
          child: Text(
            l10n.homePageDescription,
            style: FluentTheme.of(context).typography.body,
          ),
        ),
        if (!hasImported)
          Padding(
            padding: const EdgeInsets.all(32),
            child: Column(
              children: [
                const Icon(FluentIcons.people, size: 48),
                const SizedBox(height: 12),
                Text(
                  l10n.noTemplates,
                  style: FluentTheme.of(context).typography.subtitle,
                ),
                const SizedBox(height: 4),
                Text(
                  l10n.noTemplatesHint,
                  style: FluentTheme.of(context).typography.caption,
                ),
                const SizedBox(height: 12),
                FilledButton(
                  onPressed: _import,
                  child: Text(l10n.importMascot),
                ),
              ],
            ),
          )
        else
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: LayoutBuilder(
              builder: (context, constraints) {
                final wide = constraints.maxWidth >= 640;
                final library = _libraryList(context, templates);
                final details = _detailsPanel(context, selectedTemplate, state);
                if (wide) {
                  return Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Expanded(flex: 3, child: library),
                      const SizedBox(width: 10),
                      Expanded(flex: 2, child: details),
                    ],
                  );
                }
                return Column(
                  children: [library, const SizedBox(height: 8), details],
                );
              },
            ),
          ),
      ],
    );
  }

  Widget _libraryList(BuildContext context, List<LoadedMascot> templates) {
    final l10n = AppLocalizations.of(context);
    return Focus(
      autofocus: true,
      onKeyEvent: (node, event) {
        if (event is KeyDownEvent &&
            (event.logicalKey == LogicalKeyboardKey.enter ||
                event.logicalKey == LogicalKeyboardKey.numpadEnter)) {
          for (final name in _selected) {
            _spawnByName(name);
          }
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Card(
        child: Padding(
          padding: const EdgeInsets.all(10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                l10n.loadedMascots,
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 8),
              ...templates.map((template) {
                _loadPreview(template.name);
                final preview = _previews[template.name];
                final tooltip =
                    '${template.name}\nVersion: ${template.version}\nAuthor: ${template.author}';
                return Padding(
                  padding: const EdgeInsets.symmetric(vertical: 3),
                  child: GestureDetector(
                    onTap: () => _select(template.name),
                    onDoubleTap: () => _spawnByName(template.name),
                    child: HoverButton(
                      onPressed: () => _select(template.name),
                      builder: (context, hover) => Container(
                        decoration: BoxDecoration(
                          color: _isSelected(template.name)
                              ? FluentTheme.of(
                                  context,
                                ).accentColor.withValues(alpha: 0.15)
                              : hover.isHovered
                              ? FluentTheme.of(
                                  context,
                                ).resources.controlFillColorDefault
                              : Colors.transparent,
                          borderRadius: BorderRadius.circular(7),
                        ),
                        padding: const EdgeInsets.all(6),
                        child: Row(
                          children: [
                            Container(
                              width: 64,
                              height: 64,
                              decoration: BoxDecoration(
                                borderRadius: BorderRadius.circular(6),
                                color: FluentTheme.of(
                                  context,
                                ).resources.controlFillColorSecondary,
                              ),
                              clipBehavior: Clip.antiAlias,
                              child: preview == null
                                  ? const Icon(
                                      FluentIcons.image_pixel,
                                      size: 24,
                                    )
                                  : Image.memory(
                                      preview.bytes,
                                      fit: BoxFit.contain,
                                      filterQuality: FilterQuality.medium,
                                    ),
                            ),
                            const SizedBox(width: 10),
                            Expanded(
                              child: Tooltip(
                                message: tooltip,
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      template.name,
                                      style: FluentTheme.of(
                                        context,
                                      ).typography.bodyStrong,
                                    ),
                                    if (template.version.isNotEmpty)
                                      Text(
                                        'v${template.version}',
                                        style: FluentTheme.of(
                                          context,
                                        ).typography.caption,
                                      ),
                                  ],
                                ),
                              ),
                            ),
                            Checkbox(
                              checked: _isSelected(template.name),
                              onChanged: (v) =>
                                  _toggle(template.name, v ?? false),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                );
              }),
            ],
          ),
        ),
      ),
    );
  }

  Widget _detailsPanel(
    BuildContext context,
    LoadedMascot? template,
    AppState state,
  ) {
    final l10n = AppLocalizations.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (template == null)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 24),
                child: Center(
                  child: Text(
                    l10n.homeSelectTemplate,
                    style: FluentTheme.of(context).typography.body,
                  ),
                ),
              )
            else ...[
              Center(
                child: Container(
                  width: 96,
                  height: 96,
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(8),
                    color: FluentTheme.of(
                      context,
                    ).resources.controlFillColorSecondary,
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: _previews[template.name] == null
                      ? const Icon(FluentIcons.image_pixel, size: 32)
                      : Image.memory(
                          _previews[template.name]!.bytes,
                          fit: BoxFit.contain,
                        ),
                ),
              ),
              const SizedBox(height: 10),
              Center(
                child: Text(
                  template.name,
                  style: FluentTheme.of(context).typography.subtitle,
                ),
              ),
              const SizedBox(height: 12),
              _metaRow(l10n.version, template.version),
              _metaRow(l10n.author, template.author),
              const SizedBox(height: 4),
              Text(
                l10n.description,
                style: FluentTheme.of(context).typography.bodyStrong,
              ),
              const SizedBox(height: 4),
              Text(
                template.description.isEmpty ? '-' : template.description,
                style: FluentTheme.of(context).typography.caption,
              ),
              const SizedBox(height: 16),
              SizedBox(
                width: double.infinity,
                child: Button(
                  onPressed: _selected.any((n) => n != '@' && n != 'Default')
                      ? _deleteSelected
                      : null,
                  child: Text(l10n.deleteSelected),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  Widget _metaRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 80,
            child: Text(
              label,
              style: FluentTheme.of(context).typography.caption,
            ),
          ),
          Expanded(
            child: Text(
              value.isEmpty ? '-' : value,
              style: FluentTheme.of(context).typography.caption,
            ),
          ),
        ],
      ),
    );
  }
}

/// 预览字节的小包装（Image.memory 需要 Uint8List）。
class Uint8ListAware {
  Uint8ListAware(this.bytes);
  final Uint8List bytes;
}
