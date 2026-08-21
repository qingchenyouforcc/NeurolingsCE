import 'dart:io';

import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../api/runtime_api.dart';
import '../state/app_state.dart';
import '../state/settings.dart';

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

  List<String> _availableTemplates = ['@'];

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
        setState(() {
          _loading = false;
          _error = 'Runtime offline — 部分设置仅在运行时在线时可修改';
        });
        return;
      }
      final settings = await _api.command({'command': 'get_settings'});
      // settings 返回扁平 map（key->value）
      bool getBool(String k, bool def) {
        final v = settings[k];
        if (v is bool) return v;
        return def;
      }

      double getDouble(String k, double def) {
        final v = settings[k];
        if (v is num) return v.toDouble();
        return def;
      }

      int getInt(String k, int def) {
        final v = settings[k];
        if (v is num) return v.toInt();
        return def;
      }

      String getString(String k, String def) {
        final v = settings[k];
        if (v is String) return v;
        return def;
      }

      final autostartRes =
          await _api.command({'command': 'get_autostart'}).catchError((_) => <String, dynamic>{'enabled': false});
      final codexStatus =
          await _api.command({'command': 'codex_status'}).catchError((_) => <String, dynamic>{'installed': false});

      // 加载可用模板名用于下拉
      List<String> templates = ['@'];
      try {
        final list = await _api.loadedMascots();
        final names = list.map((e) => e.name).toList()..sort();
        templates = ['@', ...names];
      } catch (_) {}

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
        _startupMode = getString('startup/restoreCombinationMode', 'last');
        _autostart = (autostartRes['enabled'] as bool?) ?? false;
        _availableTemplates = templates;
        // windowed 需单独查
        // 尝试读取 windowed 状态
        _loading = false;
      });

      // 读取 windowed 模式
      try {
        final win = await _api.command({'command': 'get_settings', 'key': 'windowed'});
        // 若无则忽略，保底用 false
        if (win.containsKey('value')) {
          // ignore
        }
      } catch (_) {}
      final winRes = await _api.command({'command': 'get_settings'}).then((m) => m['windowed']).catchError((_) => null);
      if (winRes is bool) {
        setState(() => _windowed = winRes);
      }
    } catch (e) {
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
      displayInfoBar(context, builder: (ctx, close) => InfoBar(title: const Text('设置失败'), content: Text(e.toString()), severity: InfoBarSeverity.error));
    }
  }

  Future<void> _setAutostart(bool v) async {
    setState(() => _autostart = v);
    try {
      await _api.command({'command': 'set_autostart', 'enabled': v});
      if (!mounted) return;
      displayInfoBar(context, builder: (ctx, close) => InfoBar(title: Text(v ? '已开启开机自启' : '已关闭开机自启'), severity: InfoBarSeverity.success));
    } catch (e) {
      setState(() => _autostart = !v);
      if (!mounted) return;
      displayInfoBar(context, builder: (ctx, close) => InfoBar(title: const Text('自启设置失败'), content: Text(e.toString()), severity: InfoBarSeverity.error));
    }
  }

  Future<void> _setWindowed(bool v) async {
    setState(() => _windowed = v);
    try {
      await _api.command({'command': 'set_window_mode', 'enabled': v});
    } catch (e) {
      setState(() => _windowed = !v);
      if (!mounted) return;
      displayInfoBar(context, builder: (ctx, close) => InfoBar(title: Text(e.toString()), severity: InfoBarSeverity.error));
    }
  }

  Future<void> _setCodexEnabled(bool v) async {
    setState(() => _codexEnabled = v);
    try {
      await _api.command({'command': 'codex_setup', 'enabled': v});
      await _set('codex/enabled', v);
    } catch (e) {
      setState(() => _codexEnabled = !v);
      if (!mounted) return;
      displayInfoBar(context, builder: (ctx, close) => InfoBar(title: const Text('Codex 设置失败'), content: Text(e.toString()), severity: InfoBarSeverity.error));
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final settings = context.watch<SettingsController>();
    final state = context.watch<AppState>();

    if (_loading) {
      return ScaffoldPage(
        header: PageHeader(title: Text(l10n.navSettings)),
        content: const Center(child: ProgressRing()),
      );
    }

    return ScaffoldPage.scrollable(
      header: PageHeader(
        title: Text(l10n.navSettings),
        commandBar: Row(children: [
          Button(onPressed: _load, child: const Text('刷新')),
        ]),
      ),
      children: [
        if (_error != null)
          InfoBar(title: const Text('提示'), content: Text(_error!), severity: InfoBarSeverity.warning),
        if (_error != null) const SizedBox(height: 8),

        // ===== Interaction =====
        Expander(
          header: const Text('交互', style: TextStyle(fontWeight: FontWeight.bold)),
          content: Column(children: [
            _row('允许繁殖（Multiplication）', '允许桌宠通过交互产生新个体', ToggleSwitch(checked: _multiplication, onChanged: (v) { setState(() => _multiplication = v); _set('multiplicationEnabled', v); })),
            _divider(),
            _row('允许推挤窗口', '桌宠可推移活动窗口（仅支持拖动窗口）', ToggleSwitch(checked: _windowPushing, onChanged: (v) { setState(() => _windowPushing = v); _set('windowPushingEnabled', v); })),
            _divider(),
            _row('气泡', '点击桌宠时显示随机气泡', ToggleSwitch(checked: _bubbleEnabled, onChanged: (v) { setState(() => _bubbleEnabled = v); _set('speechBubbleEnabled', v); })),
            _divider(),
            _row('气泡点击次数', '连续点击 $_bubbleClicks 次触发气泡（1-10）', Row(children: [
              SizedBox(width: 200, child: Slider(value: _bubbleClicks.toDouble(), min: 1, max: 10, divisions: 9, label: '$_bubbleClicks', onChanged: (v) { setState(() => _bubbleClicks = v.round()); }, onChangeEnd: (v) => _set('speechBubbleClickCount', v.round()))),
              const SizedBox(width: 8),
              Text('$_bubbleClicks'),
            ])),
          ]),
        ),
        const SizedBox(height: 8),

        // ===== Codex =====
        Expander(
          header: const Text('Codex 集成', style: TextStyle(fontWeight: FontWeight.bold)),
          content: Column(children: [
            _row('启用 Codex 消息气泡', '在 ~/.codex/config.toml 中注入 notify 钩子', ToggleSwitch(checked: _codexEnabled, onChanged: _setCodexEnabled)),
            _divider(),
            _row('陪伴模板', '收到 Codex 通知时使用的桌宠', ComboBox<String>(value: _availableTemplates.contains(_codexTemplate) ? _codexTemplate : '@', items: _availableTemplates.map((n) => ComboBoxItem(value: n, child: Text(n == '@' ? '跟随默认（@）' : n))).toList(), onChanged: (v) { if (v == null) return; setState(() => _codexTemplate = v); _set('codex/companionTemplate', v); })),
            _divider(),
            _row('测试 Codex 消息', '发送一条测试通知验证气泡', Button(child: const Text('发送测试'), onPressed: () async { try { await _api.command({'command': 'show_codex_notification', 'payload': {'type': 'agent-turn-complete', 'state': 'completed', 'eventType': 'agentTurnComplete', 'lastAssistantMessage': 'This is a Codex test notification from NeurolingsCE.', 'sessionTitle': 'Test', 'sessionDescription': ''}}); if (!context.mounted) return; displayInfoBar(context, builder: (ctx, c) => const InfoBar(title: Text('已发送'), severity: InfoBarSeverity.success)); } catch (e) { if (!context.mounted) return; displayInfoBar(context, builder: (ctx, c) => InfoBar(title: Text(e.toString()), severity: InfoBarSeverity.error)); } })),
            _divider(),
            _row('启用 Codex 交互', '启用 AppServer 会话控制（实验性）', ToggleSwitch(checked: _codexAppServerEnabled, onChanged: (v) { setState(() => _codexAppServerEnabled = v); _set('codex/appServerEnabled', v); })),
            _divider(),
            _row('Codex 可执行文件', '留空则使用 PATH 中的 codex', Row(children: [
              Expanded(child: TextBox(controller: TextEditingController(text: _codexAppServerExecutable), placeholder: '例如 C:\\codex\\codex.exe', onChanged: (v) => _codexAppServerExecutable = v)),
              const SizedBox(width: 8),
              Button(child: const Text('保存'), onPressed: () => _set('codex/appServerExecutable', _codexAppServerExecutable)),
            ])),
            _divider(),
            _row('审批提醒气泡', '', ToggleSwitch(checked: _approvalBubble, onChanged: (v) { setState(() => _approvalBubble = v); _set('codex/approvalBubbleEnabled', v); })),
            _divider(),
            _row('计划与完成气泡', '', ToggleSwitch(checked: _planBubble, onChanged: (v) { setState(() => _planBubble = v); _set('codex/planBubbleEnabled', v); })),
          ]),
        ),
        const SizedBox(height: 8),

        // ===== Display =====
        Expander(
          header: const Text('显示', style: TextStyle(fontWeight: FontWeight.bold)),
          content: Column(children: [
            _row('窗口化模式', '在独立沙盒窗口中运行桌宠（640×480）', ToggleSwitch(checked: _windowed, onChanged: _setWindowed)),
            _divider(),
            _row('沙盒背景色', '窗口化时的画布背景（HEX #RRGGBB）', Row(children: [
              Expanded(child: TextBox(controller: TextEditingController(text: _windowedBg), placeholder: '#FF0000', onChanged: (v) => _windowedBg = v)),
              const SizedBox(width: 8),
              Button(child: const Text('保存'), onPressed: () => _set('windowedModeBackground', _windowedBg)),
            ])),
            _divider(),
            _row('缩放', '桌宠渲染缩放 ${_userScale.toStringAsFixed(2)}（0.10-10.00）', Row(children: [
              SizedBox(width: 220, child: Slider(value: _userScale, min: 0.1, max: 10.0, divisions: 99, label: _userScale.toStringAsFixed(2), onChanged: (v) => setState(() => _userScale = v), onChangeEnd: (v) => _set('userScale', double.parse(v.toStringAsFixed(3))))),
              const SizedBox(width: 8),
              Text(_userScale.toStringAsFixed(2)),
            ])),
            _divider(),
            _row('脱离速度阈值', '拖拽速度超过此值视为脱离（0-200）', Row(children: [
              SizedBox(width: 220, child: Slider(value: _detachThreshold, min: 0, max: 200, divisions: 40, label: _detachThreshold.toStringAsFixed(0), onChanged: (v) => setState(() => _detachThreshold = v), onChangeEnd: (v) => _set('detachThreshold', v))),
              const SizedBox(width: 8),
              Text(_detachThreshold.toStringAsFixed(0)),
            ])),
            _divider(),
            _row('语言', '界面语言（保存后立即生效）', ComboBox<String>(value: settings.locale, items: const [ComboBoxItem(value: 'en', child: Text('English')), ComboBoxItem(value: 'zh', child: Text('中文（简体）'))], onChanged: (v) { if (v == null) return; settings.setLocale(v); _set('language', v == 'zh' ? 'zh_CN' : 'en'); })),
          ]),
        ),
        const SizedBox(height: 8),

        // ===== Startup =====
        Expander(
          header: const Text('启动', style: TextStyle(fontWeight: FontWeight.bold)),
          content: Column(children: [
            _row('开机自启', '登录时自动启动（Windows 注册表 / Linux .desktop）', ToggleSwitch(checked: _autostart, onChanged: _setAutostart)),
            _divider(),
            _row('静默启动', '开机自启时不显示管理器窗口', ToggleSwitch(checked: _startupSilent, onChanged: (v) { setState(() => _startupSilent = v); _set('startup/silent', v); })),
            _divider(),
            _row('启动时恢复组合', '', ComboBox<String>(value: _startupMode, items: const [ComboBoxItem(value: 'none', child: Text('不恢复')), ComboBoxItem(value: 'last', child: Text('上次关闭前的组合')), ComboBoxItem(value: 'last:', child: Text('上次关闭前（兼容）'))], onChanged: (v) { if (v == null) return; setState(() => _startupMode = v); _set('startup/restoreCombinationMode', v); })),
          ]),
        ),
        const SizedBox(height: 8),

        // ===== Runtime / Storage =====
        Card(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('运行时', style: FluentTheme.of(context).typography.bodyStrong),
            const SizedBox(height: 8),
            Text(state.runtimeOnline ? '在线' : '离线', style: FluentTheme.of(context).typography.caption),
            const SizedBox(height: 4),
            Text('HTTP 127.0.0.1:32456 / IPC io.github.qingchenyouforcc.NeurolingsCE.cli', style: FluentTheme.of(context).typography.caption),
            const SizedBox(height: 8),
            Row(children: [
              Button(onPressed: state.runtimeOnline ? null : () => context.read<AppState>().startRuntime(), child: const Text('启动运行时')),
              const SizedBox(width: 8),
              Button(onPressed: () => context.read<AppState>().refresh(), child: const Text('刷新')),
            ]),
          ]),
        ),
        const SizedBox(height: 8),
        Card(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('存储', style: FluentTheme.of(context).typography.bodyStrong),
            const SizedBox(height: 8),
            SelectableText(storagePathDescription(), style: FluentTheme.of(context).typography.caption),
          ]),
        ),
      ],
    );
  }

  Widget _row(String title, String subtitle, Widget trailing) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(children: [
        Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(title),
          if (subtitle.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 2), child: Text(subtitle, style: const TextStyle(fontSize: 12, color: Color(0xFF6B6B6B)))),
        ])),
        trailing,
      ]),
    );
  }

  Widget _divider() => const Divider();
}

String storagePathDescription() {
  final home = Platform.environment['USERPROFILE'] ?? Platform.environment['HOME'] ?? '';
  if (Platform.isWindows) {
    final local = Platform.environment['LOCALAPPDATA'] ?? home;
    return '$local\\NeurolingsCE\\mascots';
  }
  return '$home/.local/share/NeurolingsCE/mascots';
}
