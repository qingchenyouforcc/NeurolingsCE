import 'package:file_picker/file_picker.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../api/runtime_api.dart';
import '../state/app_state.dart';
import '../state/settings.dart';

/// 设置页：分组行卡片（对齐原版 ManagerSettingsPage 的行清单与默认值）。
class SettingsPage extends StatefulWidget {
  const SettingsPage({super.key});

  @override
  State<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends State<SettingsPage> {
  final RuntimeApi _api = RuntimeApi();
  bool _loading = true;
  String? _error;

  // --- Interaction ---
  bool _multiplication = true;
  bool _windowPushing = false;
  bool _bubbleEnabled = true;
  int _bubbleClicks = 1;

  // --- Codex ---
  bool _codexEnabled = false;
  String _codexTemplate = '@';
  bool _codexAppServerEnabled = false;
  String _codexAppServerExecutable = '';
  bool _approvalBubble = true;
  bool _planBubble = true;

  // --- Display ---
  bool _windowed = false;
  String _windowedBg = '#FF0000';
  double _userScale = 1.0;
  double _detachThreshold = 30.0;

  // --- Startup ---
  bool _autostart = false;
  bool _startupSilent = false;
  String _startupMode = 'last';
  String _startupId = '';

  // --- Updates ---
  bool _updateCheck = true;
  String _proxyMode = 'system';
  String _proxyHost = '';
  int _proxyPort = 8080;
  String _proxyUser = '';
  String _proxyPass = '';

  List<String> _templates = ['@'];
  List<Map<String, dynamic>> _combinations = [];

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final ok = await _api.ping();
      if (!ok) {
        final l10n = AppLocalizations.of(context);
        setState(() {
          _loading = false;
          _error = l10n.settingsOfflineHint;
        });
        return;
      }
      final settings = await _api.command({'command': 'get_settings'});
      bool getBool(String k, bool def) =>
          settings[k] is bool ? settings[k] as bool : def;
      int getInt(String k, int def) =>
          settings[k] is num ? (settings[k] as num).toInt() : def;
      double getDouble(String k, double def) =>
          settings[k] is num ? (settings[k] as num).toDouble() : def;
      String getString(String k, String def) =>
          settings[k] is String ? settings[k] as String : def;

      final autostartRes = await _api
          .command({'command': 'get_autostart'})
          .catchError((_) => <String, dynamic>{'enabled': false});
      final codexStatus = await _api
          .command({'command': 'codex_status'})
          .catchError((_) => <String, dynamic>{'installed': false});
      final comboRes =
          await _api.command({'command': 'list_combinations'}).catchError(
              (_) => <String, dynamic>{'combinations': <dynamic>[]});

      List<String> templates = ['@'];
      try {
        final list = await _api.loadedMascots();
        templates = ['@', ...list.map((e) => e.name).toList()..sort()];
      } catch (_) {}

      final combos = (comboRes['combinations'] as List?)
              ?.whereType<Map>()
              .map((e) => e.cast<String, dynamic>())
              .toList() ??
          <Map<String, dynamic>>[];

      setState(() {
        _multiplication = getBool('multiplicationEnabled', true);
        _windowPushing = getBool('windowPushingEnabled', false);
        _bubbleEnabled = getBool('speechBubbleEnabled', true);
        _bubbleClicks = getInt('speechBubbleClickCount', 1).clamp(1, 10);
        _codexEnabled = getBool('codex/enabled', codexStatus['installed'] == true);
        _codexTemplate = getString('codex/companionTemplate', '@');
        _codexAppServerEnabled = getBool('codex/appServerEnabled', false);
        _codexAppServerExecutable = getString('codex/appServerExecutable', '');
        _approvalBubble = getBool('codex/approvalBubbleEnabled', true);
        _planBubble = getBool('codex/planBubbleEnabled', true);
        _userScale = getDouble('userScale', 1.0).clamp(0.1, 10.0);
        _detachThreshold = getDouble('detachThreshold', 30.0).clamp(0.0, 200.0);
        _windowedBg = getString('windowedModeBackground', '#FF0000');
        _startupSilent = getBool('startup/silent', false);
        var mode = getString('startup/restoreCombinationMode', 'last');
        // 历史值清洗（last:/id:x 归一为 last/saved+Id）。
        if (mode == 'last:') mode = 'last';
        if (mode.startsWith('id:')) {
          _startupId = mode.substring(3);
          mode = 'saved';
        } else {
          _startupId = getString('startup/restoreCombinationId', '');
        }
        _startupMode = mode;
        _autostart = (autostartRes['enabled'] as bool?) ?? false;
        _updateCheck = getBool('update/checkOnStartup', true);
        _proxyMode = getString('update/proxyMode', 'system');
        _proxyHost = getString('update/proxyHost', '');
        _proxyPort = getInt('update/proxyPort', 8080).clamp(1, 65535);
        _proxyUser = getString('update/proxyUsername', '');
        _proxyPass = getString('update/proxyPassword', '');
        _templates = templates;
        _combinations = combos;
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

  Future<void> _set(String key, dynamic value) async {
    try {
      await _api.command({
        'command': 'set_settings',
        'key': key,
        'value': value,
      });
    } catch (e) {
      if (!mounted) return;
      final l10n = AppLocalizations.of(context);
      displayInfoBar(context, builder: (ctx, close) {
        return InfoBar(
            title: Text(l10n.settingsSaveFailed),
            content: Text(e.toString()),
            severity: InfoBarSeverity.error);
      });
    }
  }

  Future<void> _setAutostart(bool v) async {
    final l10n = AppLocalizations.of(context);
    setState(() => _autostart = v);
    try {
      await _api.command({'command': 'set_autostart', 'enabled': v});
    } catch (e) {
      setState(() => _autostart = !v);
      if (!mounted) return;
      displayInfoBar(context, builder: (ctx, close) {
        return InfoBar(
            title: Text(l10n.error),
            content: Text(e.toString()),
            severity: InfoBarSeverity.error);
      });
    }
  }

  Future<void> _setWindowed(bool v) async {
    setState(() => _windowed = v);
    try {
      await _api.command({'command': 'set_window_mode', 'enabled': v});
    } catch (e) {
      setState(() => _windowed = !v);
    }
  }

  /// 开启 Codex 通知前弹确认对话框（对齐原版：显示配置路径与命令）。
  Future<void> _setCodexEnabled(bool v) async {
    final l10n = AppLocalizations.of(context);
    if (v) {
      final status = await _api
          .command({'command': 'codex_status'})
          .catchError((_) => <String, dynamic>{});
      final config = status['config'] ?? '~/.codex/config.toml';
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (dialogContext) => ContentDialog(
          title: Text(l10n.settingsCodexConfirmTitle),
          content: Text(l10n.settingsCodexConfirmBody('$config')),
          actions: [
            Button(
              onPressed: () => Navigator.pop(dialogContext, false),
              child: Text(l10n.cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, true),
              child: Text(l10n.ok),
            ),
          ],
        ),
      );
      if (confirmed != true) return;
    }
    setState(() => _codexEnabled = v);
    try {
      await _api.command({'command': 'codex_setup', 'enabled': v});
      await _set('codex/enabled', v);
    } catch (e) {
      setState(() => _codexEnabled = !v);
      if (!mounted) return;
      displayInfoBar(context, builder: (ctx, close) {
        return InfoBar(
            title: Text(l10n.error),
            content: Text(e.toString()),
            severity: InfoBarSeverity.error);
      });
    }
  }

  Future<void> _browseExecutable() async {
    final picked = await FilePicker.platform.pickFiles(
      dialogTitle: AppLocalizations.of(context).settingsCodexExecutable,
    );
    if (picked == null) return;
    final path = picked.files.singleOrNull?.path;
    if (path == null) return;
    setState(() => _codexAppServerExecutable = path);
    await _set('codex/appServerExecutable', path);
  }

  Future<void> _pickBackgroundColor() async {
    final l10n = AppLocalizations.of(context);
    final picked = await showDialog<String>(
      context: context,
      builder: (dialogContext) => _ColorPickerDialog(initial: _windowedBg),
    );
    if (picked == null) return;
    setState(() => _windowedBg = picked);
    await _set('windowedModeBackground', picked);
    if (!mounted) return;
    displayInfoBar(context, builder: (ctx, close) {
      return InfoBar(title: Text(l10n.settingsColorSaved), severity: InfoBarSeverity.success);
    });
  }

  Future<void> _pickScale() async {
    final l10n = AppLocalizations.of(context);
    final picked = await showDialog<double>(
      context: context,
      builder: (dialogContext) => _ScaleDialog(initial: _userScale),
    );
    if (picked == null) return;
    setState(() => _userScale = picked);
    await _set('userScale', picked);
    if (!mounted) return;
    displayInfoBar(context,
        builder: (ctx, close) => InfoBar(
            title: Text(l10n.settingsScaleSaved),
            severity: InfoBarSeverity.success));
  }

  Future<void> _pickDetach() async {
    final l10n = AppLocalizations.of(context);
    final picked = await showDialog<double>(
      context: context,
      builder: (dialogContext) => _DetachDialog(initial: _detachThreshold),
    );
    if (picked == null) return;
    setState(() => _detachThreshold = picked);
    await _set('detachThreshold', picked);
    if (!mounted) return;
    displayInfoBar(context,
        builder: (ctx, close) => InfoBar(
            title: Text(l10n.settingsDetachSaved),
            severity: InfoBarSeverity.success));
  }

  Future<void> _pickProxy() async {
    final l10n = AppLocalizations.of(context);
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => _ProxyDialog(
        mode: _proxyMode,
        host: _proxyHost,
        port: _proxyPort,
        user: _proxyUser,
        pass: _proxyPass,
        onSave: (mode, host, port, user, pass) async {
          setState(() {
            _proxyMode = mode;
            _proxyHost = host;
            _proxyPort = port;
            _proxyUser = user;
            _proxyPass = pass;
          });
          await _set('update/proxyMode', mode);
          await _set('update/proxyHost', host);
          await _set('update/proxyPort', port);
          await _set('update/proxyUsername', user);
          await _set('update/proxyPassword', pass);
        },
        l10n: l10n,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final settings = context.watch<SettingsController>();
    if (_loading) {
      return ScaffoldPage(
        header: PageHeader(title: Text(l10n.navSettings)),
        content: const Center(child: ProgressRing()),
      );
    }

    final missingTemplate =
        !_templates.contains(_codexTemplate) && _codexTemplate != '@';

    return ScaffoldPage.scrollable(
      header: PageHeader(
        title: Text(l10n.navSettings),
        commandBar: Row(children: [
          Button(onPressed: _load, child: Text(l10n.refresh)),
        ]),
      ),
      children: [
        if (_error != null)
          InfoBar(
              title: Text(l10n.error),
              content: Text(_error!),
              severity: InfoBarSeverity.warning),
        if (_error != null) const SizedBox(height: 8),

        // ===== Interaction =====
        _group(context, l10n.settingsGroupInteraction, [
          _toggle(l10n.settingsMultiplication, l10n.settingsMultiplicationHint,
              _multiplication, (v) {
            setState(() => _multiplication = v);
            _set('multiplicationEnabled', v);
          }),
          _toggle(l10n.settingsWindowPushing, l10n.settingsWindowPushingHint,
              _windowPushing, (v) {
            setState(() => _windowPushing = v);
            _set('windowPushingEnabled', v);
          }),
          _toggle(l10n.settingsSpeechBubble, l10n.settingsSpeechBubbleHint,
              _bubbleEnabled, (v) {
            setState(() => _bubbleEnabled = v);
            _set('speechBubbleEnabled', v);
          }),
          _row(
            l10n.settingsBubbleClicks,
            l10n.settingsBubbleClicksHint(_bubbleClicks),
            NumberBox<int>(
              value: _bubbleClicks,
              min: 1,
              max: 10,
              mode: SpinButtonPlacementMode.inline,
              onChanged: (v) {
                if (v == null) return;
                setState(() => _bubbleClicks = v);
                _set('speechBubbleClickCount', v);
              },
            ),
          ),
        ]),

        // ===== Codex =====
        _group(context, l10n.settingsGroupCodex, [
          _toggle(l10n.settingsCodexEnabled, l10n.settingsCodexEnabledHint,
              _codexEnabled, _setCodexEnabled),
          _row(
            l10n.settingsCodexTemplate,
            missingTemplate
                ? l10n.settingsCodexTemplateMissing(_codexTemplate)
                : l10n.settingsCodexTemplateHint,
            ComboBox<String>(
              value: _templates.contains(_codexTemplate) ? _codexTemplate : '@',
              items: [
                ComboBoxItem(
                    value: '@', child: Text(l10n.settingsCodexTemplateDefault)),
                ..._templates
                    .where((n) => n != '@')
                    .map((n) => ComboBoxItem(value: n, child: Text(n))),
                if (missingTemplate)
                  ComboBoxItem(
                      value: _codexTemplate, child: Text(_codexTemplate)),
              ],
              onChanged: (v) {
                if (v == null) return;
                setState(() => _codexTemplate = v);
                _set('codex/companionTemplate', v);
              },
            ),
          ),
          _row(
              l10n.settingsCodexTest,
              l10n.settingsCodexTestHint,
              Button(
                onPressed: _codexEnabled ? () async {
                  try {
                    await _api.command({
                      'command': 'show_codex_notification',
                      'payload': {
                        'type': 'agent-turn-complete',
                        'state': 'completed',
                        'eventType': 'agentTurnComplete',
                        'lastAssistantMessage':
                            'This is a Codex test notification from NeurolingsCE.',
                        'sessionTitle': 'Test',
                        'sessionDescription': '',
                      }
                    });
                  } catch (_) {}
                } : null,
                child: Text(l10n.settingsCodexTestSend))),
          _toggle(l10n.settingsCodexAppServer, l10n.settingsCodexAppServerHint,
              _codexAppServerEnabled, (v) {
            setState(() => _codexAppServerEnabled = v);
            _set('codex/appServerEnabled', v);
          }),
          _row(
            l10n.settingsCodexExecutable,
            l10n.settingsCodexExecutableHint,
            Row(children: [
              SizedBox(
                width: 220,
                child: TextBox(
                  controller:
                      TextEditingController(text: _codexAppServerExecutable),
                  onChanged: (v) => _codexAppServerExecutable = v,
                ),
              ),
              const SizedBox(width: 8),
              Button(onPressed: _browseExecutable, child: Text(l10n.settingsBrowse)),
              const SizedBox(width: 8),
              Button(
                onPressed: () =>
                    _set('codex/appServerExecutable', _codexAppServerExecutable),
                child: Text(l10n.save),
              ),
            ]),
          ),
          _toggle(l10n.settingsApprovalBubble, '', _approvalBubble, (v) {
            setState(() => _approvalBubble = v);
            _set('codex/approvalBubbleEnabled', v);
          }),
          _toggle(l10n.settingsPlanBubble, '', _planBubble, (v) {
            setState(() => _planBubble = v);
            _set('codex/planBubbleEnabled', v);
          }),
          _row(
              l10n.settingsDetachSpeed,
              l10n.settingsDetachSpeedHint(_detachThreshold.toStringAsFixed(0)),
              Button(onPressed: _pickDetach, child: Text(l10n.settingsEdit))),
        ]),

        // ===== Display =====
        _group(context, l10n.settingsGroupDisplay, [
          _toggle(l10n.settingsWindowedMode, l10n.settingsWindowedModeHint,
              _windowed, _setWindowed),
          _row(
              l10n.settingsWindowedBg,
              l10n.settingsWindowedBgHint,
              Button(
                  onPressed: _pickBackgroundColor,
                  child: Text(_windowedBg))),
          _row(
              l10n.settingsScale,
              l10n.settingsScaleHint(_userScale.toStringAsFixed(2)),
              Button(onPressed: _pickScale, child: Text(l10n.settingsEdit))),
          _row(
              l10n.settingsLanguage,
              l10n.settingsLanguageHint,
              ComboBox<String>(
                value: settings.locale,
                items: const [
                  ComboBoxItem(value: 'en', child: Text('English')),
                  ComboBoxItem(value: 'zh', child: Text('中文（简体）')),
                ],
                onChanged: (v) {
                  if (v == null) return;
                  settings.setLocale(v);
                  _set('language', v == 'zh' ? 'zh_CN' : 'en');
                },
              )),
        ]),

        // ===== Startup =====
        _group(context, l10n.settingsGroupStartup, [
          _toggle(l10n.settingsAutostart, l10n.settingsAutostartHint, _autostart,
              _setAutostart),
          _toggle(l10n.settingsSilent, l10n.settingsSilentHint, _startupSilent,
              (v) {
            setState(() => _startupSilent = v);
            _set('startup/silent', v);
          }),
          _row(
            l10n.settingsStartupCombo,
            _startupModeLabel(l10n),
            ComboBox<String>(
              value: _startupMode == 'none' ? 'none' : (_startupMode == 'saved' ? 'saved' : 'last'),
              items: [
                ComboBoxItem(value: 'last', child: Text(l10n.settingsStartupLast)),
                ComboBoxItem(value: 'none', child: Text(l10n.settingsStartupNone)),
                if (_combinations.isNotEmpty)
                  ComboBoxItem(
                      value: 'saved', child: Text(l10n.settingsStartupSaved)),
              ],
              onChanged: (v) {
                if (v == null) return;
                if (v == 'saved') {
                  // 弹出组合选择对话框。
                  showDialog<Map<String, dynamic>>(
                    context: context,
                    builder: (ctx) => ContentDialog(
                      title: Text(l10n.settingsStartupChoose),
                      content: SizedBox(
                        height: 240,
                        child: ListView(
                          children: _combinations
                              .map((c) => ListTile(
                                    title: Text(c['name'] ?? c['id']),
                                    onPressed: () => Navigator.pop(ctx, c),
                                  ))
                              .toList(),
                        ),
                      ),
                      actions: [
                        Button(
                            onPressed: () => Navigator.pop(ctx),
                            child: Text(l10n.cancel)),
                      ],
                    ),
                  ).then((chosen) {
                    if (chosen == null) return;
                    final id = chosen['id'] as String? ?? '';
                    setState(() {
                      _startupMode = 'saved';
                      _startupId = id;
                    });
                    _set('startup/restoreCombinationMode', 'saved');
                    _set('startup/restoreCombinationId', id);
                  });
                } else {
                  setState(() => _startupMode = v);
                  _set('startup/restoreCombinationMode', v);
                  _set('startup/restoreCombinationId', '');
                }
              },
            ),
          ),
        ]),

        // ===== Updates =====
        _group(context, l10n.settingsGroupUpdates, [
          _toggle(l10n.settingsUpdateCheck, l10n.settingsUpdateCheckHint,
              _updateCheck, (v) {
            setState(() => _updateCheck = v);
            _set('update/checkOnStartup', v);
          }),
          _row(
              l10n.settingsUpdateProxy,
              l10n.settingsUpdateProxyHint(_proxyModeLabel(l10n)),
              Button(onPressed: _pickProxy, child: Text(l10n.settingsConfigure))),
        ]),
      ],
    );
  }

  String _proxyModeLabel(AppLocalizations l10n) {
    switch (_proxyMode) {
      case 'direct':
        return l10n.settingsProxyDirect;
      case 'http':
        return l10n.settingsProxyHttp;
      case 'socks5':
        return l10n.settingsProxySocks5;
      default:
        return l10n.settingsProxySystem;
    }
  }

  String _startupModeLabel(AppLocalizations l10n) {
    if (_startupMode == 'none') return l10n.settingsStartupNone;
    if (_startupMode == 'saved') {
      final match = _combinations
          .where((c) => c['id'] == _startupId)
          .firstOrNull;
      return l10n.settingsStartupSavedNamed(
          match?['name'] as String? ?? _startupId);
    }
    return l10n.settingsStartupLast;
  }

  Widget _group(BuildContext context, String title, List<Widget> rows) {
    final children = <Widget>[];
    for (var i = 0; i < rows.length; i++) {
      children.add(rows[i]);
      if (i != rows.length - 1) children.add(const Divider());
    }
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Expander(
        header: Text(title, style: const TextStyle(fontWeight: FontWeight.bold)),
        content: Column(children: children),
      ),
    );
  }

  Widget _row(String title, String subtitle, Widget trailing) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(children: [
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title),
            if (subtitle.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 2),
                child: Text(subtitle,
                    style: const TextStyle(fontSize: 12, color: Color(0xFF6B6B6B))),
              ),
          ]),
        ),
        trailing,
      ]),
    );
  }

  Widget _toggle(String title, String subtitle, bool value, ValueChanged<bool> onChanged) {
    return _row(title, subtitle, ToggleSwitch(checked: value, onChanged: onChanged));
  }
}

/// 背景色选择对话框（预设色板 + HEX 输入，近似原版 CompactFluentColorDialog）。
class _ColorPickerDialog extends StatefulWidget {
  const _ColorPickerDialog({required this.initial});
  final String initial;

  @override
  State<_ColorPickerDialog> createState() => _ColorPickerDialogState();
}

class _ColorPickerDialogState extends State<_ColorPickerDialog> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initial);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    const presets = [
      '#FF0000', '#FF7F00', '#FFFF00', '#00FF00', '#00FFFF', '#0000FF',
      '#8B00FF', '#FF00FF', '#FFFFFF', '#C0C0C0', '#808080', '#000000',
    ];
    return ContentDialog(
      title: Text(AppLocalizations.of(context).settingsWindowedBg),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: presets
                .map((hex) => GestureDetector(
                      onTap: () => Navigator.pop(context, hex),
                      child: Container(
                        width: 36,
                        height: 36,
                        decoration: BoxDecoration(
                          color: _parseColor(hex),
                          border: Border.all(color: Colors.grey),
                          borderRadius: BorderRadius.circular(6),
                        ),
                      ),
                    ))
                .toList(),
          ),
          const SizedBox(height: 12),
          TextBox(
            controller: _controller,
            placeholder: '#RRGGBB',
          ),
        ],
      ),
      actions: [
        Button(
          onPressed: () => Navigator.pop(context),
          child: Text(AppLocalizations.of(context).cancel),
        ),
        FilledButton(
          onPressed: () {
            final text = _controller.text.trim();
            if (RegExp(r'^#[0-9a-fA-F]{6}$').hasMatch(text)) {
              Navigator.pop(context, text.toUpperCase());
            }
          },
          child: Text(AppLocalizations.of(context).ok),
        ),
      ],
    );
  }

  Color _parseColor(String hex) {
    final value = int.tryParse(hex.replaceFirst('#', '0xFF'));
    return Color(value ?? 0xFFFF0000);
  }
}

/// 缩放选择对话框（0.10–10.00，步进 0.05，对齐原版滑杆+数值）。
class _ScaleDialog extends StatefulWidget {
  const _ScaleDialog({required this.initial});
  final double initial;

  @override
  State<_ScaleDialog> createState() => _ScaleDialogState();
}

class _ScaleDialogState extends State<_ScaleDialog> {
  late double _value;

  @override
  void initState() {
    super.initState();
    _value = widget.initial;
  }

  @override
  Widget build(BuildContext context) {
    // 千分位映射（原版 slider 100–10000 ‰）。
    final perMille = (_value * 1000).round().clamp(100, 10000);
    return ContentDialog(
      title: Text(AppLocalizations.of(context).settingsScale),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(_value.toStringAsFixed(2)),
          Slider(
            value: perMille.toDouble(),
            min: 100,
            max: 10000,
            divisions: 198,
            onChanged: (v) => setState(
                () => _value = ((v / 10).round()) / 100.0),
          ),
        ],
      ),
      actions: [
        Button(
            onPressed: () => Navigator.pop(context),
            child: Text(AppLocalizations.of(context).cancel)),
        FilledButton(
            onPressed: () => Navigator.pop(context, _value),
            child: Text(AppLocalizations.of(context).ok)),
      ],
    );
  }
}

/// 脱离速度阈值对话框（0–200）。
class _DetachDialog extends StatefulWidget {
  const _DetachDialog({required this.initial});
  final double initial;

  @override
  State<_DetachDialog> createState() => _DetachDialogState();
}

class _DetachDialogState extends State<_DetachDialog> {
  late double _value;

  @override
  void initState() {
    super.initState();
    _value = widget.initial;
  }

  @override
  Widget build(BuildContext context) {
    return ContentDialog(
      title: Text(AppLocalizations.of(context).settingsDetachSpeed),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(_value.toStringAsFixed(0)),
          Slider(
            value: _value,
            min: 0,
            max: 200,
            divisions: 200,
            onChanged: (v) => setState(() => _value = v),
          ),
        ],
      ),
      actions: [
        Button(
            onPressed: () => Navigator.pop(context),
            child: Text(AppLocalizations.of(context).cancel)),
        FilledButton(
            onPressed: () => Navigator.pop(context, _value),
            child: Text(AppLocalizations.of(context).ok)),
      ],
    );
  }
}

/// 代理配置对话框（system/direct/http/socks5 + 主机/端口/账号）。
class _ProxyDialog extends StatefulWidget {
  const _ProxyDialog({
    required this.mode,
    required this.host,
    required this.port,
    required this.user,
    required this.pass,
    required this.onSave,
    required this.l10n,
  });

  final String mode;
  final String host;
  final int port;
  final String user;
  final String pass;
  final Future<void> Function(String, String, int, String, String) onSave;
  final AppLocalizations l10n;

  @override
  State<_ProxyDialog> createState() => _ProxyDialogState();
}

class _ProxyDialogState extends State<_ProxyDialog> {
  late String _mode;
  late final TextEditingController _host;
  late final TextEditingController _port;
  late final TextEditingController _user;
  late final TextEditingController _pass;

  @override
  void initState() {
    super.initState();
    _mode = widget.mode;
    _host = TextEditingController(text: widget.host);
    _port = TextEditingController(text: widget.port.toString());
    _user = TextEditingController(text: widget.user);
    _pass = TextEditingController(text: widget.pass);
  }

  @override
  void dispose() {
    _host.dispose();
    _port.dispose();
    _user.dispose();
    _pass.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = widget.l10n;
    return ContentDialog(
      title: Text(l10n.settingsUpdateProxy),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          ComboBox<String>(
            value: _mode,
            items: [
              ComboBoxItem(value: 'system', child: Text(l10n.settingsProxySystem)),
              ComboBoxItem(value: 'direct', child: Text(l10n.settingsProxyDirect)),
              ComboBoxItem(value: 'http', child: Text(l10n.settingsProxyHttp)),
              ComboBoxItem(value: 'socks5', child: Text(l10n.settingsProxySocks5)),
            ],
            onChanged: (v) {
              if (v != null) setState(() => _mode = v);
            },
          ),
          const SizedBox(height: 10),
          TextBox(controller: _host, placeholder: l10n.settingsProxyHost),
          const SizedBox(height: 8),
          TextBox(controller: _port, placeholder: l10n.settingsProxyPort),
          const SizedBox(height: 8),
          TextBox(controller: _user, placeholder: l10n.settingsProxyUser),
          const SizedBox(height: 8),
          PasswordBox(controller: _pass, placeholder: l10n.settingsProxyPass),
        ],
      ),
      actions: [
        Button(onPressed: () => Navigator.pop(context), child: Text(l10n.cancel)),
        FilledButton(
          onPressed: () {
            widget.onSave(_mode, _host.text.trim(),
                int.tryParse(_port.text.trim()) ?? 8080, _user.text, _pass.text);
            Navigator.pop(context);
          },
          child: Text(l10n.ok),
        ),
      ],
    );
  }
}
