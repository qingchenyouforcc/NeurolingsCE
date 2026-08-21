import 'dart:io';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:path/path.dart' as path;
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

/// Codex page: manage AI companion integration, wired to the runtime
/// codex_status / codex_setup commands over POST /command.
class CodexPage extends StatefulWidget {
  const CodexPage({super.key});

  @override
  State<CodexPage> createState() => _CodexPageState();
}

class _CodexPageState extends State<CodexPage> {
  bool _installed = false;
  bool _loading = true;
  bool _busy = false;
  String? _error;

  String get _configPath {
    final home = Platform.isWindows
        ? Platform.environment['USERPROFILE']
        : Platform.environment['HOME'];
    return path.join(home ?? '', '.codex', 'config.toml');
  }

  Future<void> _loadStatus() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final state = context.read<AppState>();
      final result = await state.api.command({'command': 'codex_status'});
      if (!mounted) return;
      setState(() {
        _installed = result['installed'] == true;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  Future<void> _toggle(bool value) async {
    setState(() => _busy = true);
    try {
      final state = context.read<AppState>();
      final result = await state.api
          .command({'command': 'codex_setup', 'enabled': value});
      if (!mounted) return;
      setState(() {
        _installed = result['enabled'] == true;
        _busy = false;
      });
      await displayInfoBar(context, builder: (context, close) {
        return InfoBar(
          title: Text(value ? '已启用 Codex 通知' : '已禁用 Codex 通知'),
          content: Text(
              '配置已写入 ${result['config'] ?? _configPath}'),
          severity: InfoBarSeverity.success,
          action: IconButton(
            icon: const Icon(FluentIcons.clear),
            onPressed: close,
          ),
        );
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _busy = false);
      await displayInfoBar(context, builder: (context, close) {
        return InfoBar(
          title: const Text('操作失败'),
          content: Text(e.toString()),
          severity: InfoBarSeverity.error,
          action: IconButton(
            icon: const Icon(FluentIcons.clear),
            onPressed: close,
          ),
        );
      });
    }
  }

  void _copyConfigPath() {
    Clipboard.setData(ClipboardData(text: _configPath));
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadStatus());
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);

    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.navCodex)),
      children: [
        Card(
          margin: const EdgeInsets.all(16),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Codex 通知钩子',
                    style: FluentTheme.of(context).typography.bodyLarge,
                  ),
                  const SizedBox(height: 16),
                  if (_loading)
                    const Row(children: [
                      ProgressRing(),
                      SizedBox(width: 12),
                      Text('正在读取配置...'),
                    ])
                  else if (_error != null)
                    InfoBar(
                      title: const Text('无法读取运行时状态'),
                      content: Text(
                          '${_error!}\n请先在主页启动运行时，然后点击刷新。'),
                      severity: InfoBarSeverity.warning,
                    )
                  else
                    Row(children: [
                      ToggleSwitch(
                        checked: _installed,
                        onChanged: _busy ? null : _toggle,
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              _installed ? '已启用' : '已禁用',
                              style: FluentTheme.of(context)
                                  .typography
                                  .bodyStrong,
                            ),
                            Text(
                              _installed
                                  ? 'Codex 会话事件将通过 --codex-notify 通知桌宠'
                                  : '启用后 Codex 的 notify 钩子会接到 NeurolingsCE-cli',
                              style:
                                  FluentTheme.of(context).typography.caption,
                            ),
                          ],
                        ),
                      ),
                      IconButton(
                        icon: const Icon(FluentIcons.refresh),
                        onPressed: _busy ? null : _loadStatus,
                      ),
                    ]),
                  const SizedBox(height: 24),
                  Text(
                    '配置文件',
                    style: FluentTheme.of(context).typography.bodyLarge,
                  ),
                  const SizedBox(height: 8),
                  Row(children: [
                    const Icon(FluentIcons.code),
                    const SizedBox(width: 8),
                    Expanded(child: SelectableText(_configPath)),
                    IconButton(
                      icon: const Icon(FluentIcons.copy),
                      onPressed: _copyConfigPath,
                    ),
                  ]),
                ]),
          ),
        ),
        Card(
          margin: const EdgeInsets.symmetric(horizontal: 16),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(children: [
                    const Icon(FluentIcons.info),
                    const SizedBox(width: 8),
                    Text(
                      '工作原理',
                      style: FluentTheme.of(context).typography.bodyStrong,
                    ),
                  ]),
                  const SizedBox(height: 12),
                  Text(
                    '启用后，NeurolingsCE 会在 ~/.codex/config.toml 中写入一个标记块：\n\n'
                    '# >>> NeurolingsCE notify >>>\n'
                    'notify = ["NeurolingsCE-cli", "--codex-notify"]\n'
                    '# <<< NeurolingsCE notify <<<\n\n'
                    '当 Codex 产生会话事件时，该钩子会调用 CLI，运行时随后在伴生'
                    '桌宠上显示气泡。禁用时标记块会被完整移除，不影响其他配置。',
                    style: FluentTheme.of(context).typography.body,
                  ),
                ]),
          ),
        ),
      ],
    );
  }
}
