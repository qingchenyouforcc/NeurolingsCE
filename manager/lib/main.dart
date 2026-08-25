import 'dart:async';
import 'dart:io';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:provider/provider.dart';
import 'package:window_manager/window_manager.dart';

import 'api/runtime_api.dart';
import 'pages/about_page.dart';
import 'pages/codex_page.dart';
import 'pages/combinations_page.dart';
import 'pages/create_page.dart';
import 'pages/home_page.dart';
import 'pages/settings_page.dart';
import 'pages/store_page.dart';
import 'state/app_state.dart';
import 'state/settings.dart';

/// Manager 主窗口标题（runtime 的托盘显隐控制按此标题定位窗口）。
const String kManagerWindowTitle = 'NeurolingsCE — Mascot Manager';

/// 构造管理器窗口心跳载荷。
///
/// [x]、[y]、[width] 与 [height] 使用窗口的逻辑像素坐标，[isVisible] 表示窗口级可见性。
Map<String, dynamic> buildManagerHeartbeatPayload({
  required int x,
  required int y,
  required int width,
  required int height,
  required bool isVisible,
}) {
  return {
    'command': 'manager_heartbeat',
    'x': x,
    'y': y,
    'width': width,
    'height': height,
    'is_visible': isVisible,
  };
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
    await windowManager.ensureInitialized();
    const options = WindowOptions(
      size: Size(1000, 680),
      minimumSize: Size(400, 450),
      title: kManagerWindowTitle,
    );
    await windowManager.waitUntilReadyToShow(options, () async {
      // 对齐原版：隐藏最大化按钮（ElaWindow setWindowButtonFlag）。
      await windowManager.setMaximizable(false);
      await windowManager.show();
      await windowManager.focus();
    });
  }
  runApp(const ManagerApp());
}

class ManagerApp extends StatelessWidget {
  const ManagerApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => AppState()),
        ChangeNotifierProvider<SettingsController>(
          create: (_) => SettingsController(),
        ),
      ],
      child: Consumer<SettingsController>(
        builder: (context, settings, _) {
          return FluentApp(
            title: kManagerWindowTitle,
            debugShowCheckedModeBanner: false,
            locale: Locale(settings.locale),
            supportedLocales: const [Locale('en'), Locale('zh')],
            localizationsDelegates: const [
              AppLocalizations.delegate,
              GlobalMaterialLocalizations.delegate,
              GlobalWidgetsLocalizations.delegate,
              GlobalCupertinoLocalizations.delegate,
            ],
            theme: FluentThemeData(brightness: Brightness.light),
            darkTheme: FluentThemeData(brightness: Brightness.dark),
            themeMode: ThemeMode.system,
            home: const ManagerShell(),
          );
        },
      ),
    );
  }
}

class ManagerShell extends StatefulWidget {
  const ManagerShell({super.key});

  @override
  State<ManagerShell> createState() => _ManagerShellState();
}

class _ManagerShellState extends State<ManagerShell> with WindowListener {
  int _index = 0;
  Timer? _heartbeat;
  Timer? _inspectTimer;
  int? _inspectId;
  String _inspectName = '';
  String _inspectText = '';

  @override
  void initState() {
    super.initState();
    windowManager.addListener(this);
    windowManager.setPreventClose(true);
    // 定期上报窗口矩形与可见性：召唤落点跟随管理器所在屏；
    // 响应携带"跳转 Codex 页"请求（点击 Codex 气泡触发）。
    _heartbeat = Timer.periodic(
      const Duration(seconds: 1),
      (_) => _reportWindowRect(),
    );
    // 启动即确保运行时在线（对齐原版"Manager 即运行时"的体感）。
    WidgetsBinding.instance.addPostFrameCallback((_) => _ensureRuntime());
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (Platform.environment['FLUTTER_TEST'] == 'true') return;
    windowManager.setTitle(AppLocalizations.of(context).appTitle);
  }

  @override
  void dispose() {
    _heartbeat?.cancel();
    _inspectTimer?.cancel();
    windowManager.removeListener(this);
    super.dispose();
  }

  Future<void> _ensureRuntime() async {
    // flutter test 环境禁止拉起真实进程。
    if (Platform.environment['FLUTTER_TEST'] == 'true') return;
    final state = context.read<AppState>();
    await state.refresh();
    if (!state.runtimeOnline) {
      await startRuntimeProcess();
      for (var i = 0; i < 10 && mounted; i++) {
        await Future.delayed(const Duration(milliseconds: 500));
        if (!mounted) return;
        await state.refresh();
        if (state.runtimeOnline) break;
      }
    }
  }

  Future<void> _reportWindowRect() async {
    try {
      final state = context.read<AppState>();
      final position = await windowManager.getPosition();
      final size = await windowManager.getSize();
      final isVisible = await windowManager.isVisible();
      final response = await state.api.command(
        buildManagerHeartbeatPayload(
          x: position.dx.round(),
          y: position.dy.round(),
          width: size.width.round(),
          height: size.height.round(),
          isVisible: isVisible,
        ),
      );
      if (response['codex_navigate'] == true && mounted) {
        // 点击 Codex 气泡：跳转 Codex 页（索引 4，对齐原版 showCodexPage）。
        setState(() => _index = 4);
      }
      if (response['update_navigate'] == true && mounted) {
        // 启动检查发现新版本：跳转 About 页（对齐原版托盘通知点击行为）。
        setState(() => _index = 6);
      }
      state.applyHeartbeat(response);
      final inspectId = response['inspect_id'];
      if (inspectId is num && mounted) {
        _openInspector(inspectId.toInt());
      }
    } catch (_) {
      // 运行时离线时静默跳过。
    }
  }

  void _openInspector(int id) {
    _inspectTimer?.cancel();
    setState(() {
      _inspectId = id;
      _inspectText = '';
      _inspectName = '';
    });
    _refreshInspector();
    _inspectTimer = Timer.periodic(
      const Duration(milliseconds: 250),
      (_) => _refreshInspector(),
    );
  }

  Future<void> _refreshInspector() async {
    final id = _inspectId;
    if (id == null || !mounted) return;
    try {
      final result = await context.read<AppState>().api.command({
        'command': 'inspect_mascot',
        'id': id,
      });
      if (!mounted) return;
      setState(() {
        _inspectName = (result['name'] as String?) ?? '';
        _inspectText = (result['text'] as String?) ?? '';
      });
    } catch (_) {
      if (!mounted) return;
      _inspectTimer?.cancel();
      setState(() {
        _inspectId = null;
        _inspectText = '';
      });
    }
  }

  void _closeInspector() {
    _inspectTimer?.cancel();
    context.read<AppState>().clearInspect();
    setState(() {
      _inspectId = null;
      _inspectText = '';
    });
  }

  Future<void> _importDropped(List<String> paths) async {
    if (paths.isEmpty) return;
    final state = context.read<AppState>();
    final l10n = AppLocalizations.of(context);
    for (final path in paths) {
      final output = await state.importArchive(path);
      if (!mounted) return;
      displayInfoBar(
        context,
        builder: (ctx, close) {
          return InfoBar(
            title: Text(
              output.contains('imported') || output.startsWith('{')
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

  @override
  void onWindowClose() async {
    final state = context.read<AppState>();
    await state.refresh();
    if (!mounted) return;
    final l10n = AppLocalizations.of(context);
    if (state.running.isNotEmpty) {
      // 对齐原版 Windows 行为：有桌宠在运行时关闭按钮仅隐藏窗口。
      await windowManager.hide();
      return;
    }
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => ContentDialog(
        title: Text(l10n.closeConfirmTitle),
        content: Text(l10n.closeConfirmBody),
        actions: [
          Button(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: Text(l10n.closeKeepOpen),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: Text(l10n.closeConfirmClose),
          ),
        ],
      ),
    );
    if (confirmed == true) {
      // 对齐原版退出语义：保存组合并停止运行时。
      try {
        await state.api.command({'command': 'stop_runtime'});
      } catch (_) {
        // 运行时可能已离线。
      }
      await windowManager.destroy();
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final state = context.watch<AppState>();
    final pages = <Widget>[
      const HomePage(),
      const StorePage(),
      const CreatePage(),
      const CombinationsPage(),
      const CodexPage(),
      const SettingsPage(),
      const AboutPage(),
    ];
    // 主区 5 项 + 底部 2 项，对齐原版 addPageNode vs addFooterNode。
    final navigation = NavigationView(
      pane: NavigationPane(
        selected: _index,
        onChanged: (index) => setState(() => _index = index),
        displayMode: PaneDisplayMode.auto,
        items: [
          PaneItem(
            icon: const Icon(FluentIcons.home),
            title: Text(l10n.navHome),
            body: pages[0],
          ),
          PaneItem(
            icon: const Icon(FluentIcons.shop),
            title: Text(l10n.navStore),
            body: pages[1],
          ),
          PaneItem(
            icon: const Icon(FluentIcons.fabric_new_folder),
            title: Text(l10n.navCreate),
            body: pages[2],
          ),
          PaneItem(
            icon: const Icon(FluentIcons.group),
            title: Text(l10n.navCombinations),
            body: pages[3],
          ),
          PaneItem(
            icon: const Icon(FluentIcons.robot),
            title: Text(l10n.navCodex),
            body: pages[4],
          ),
        ],
        footerItems: [
          PaneItem(
            icon: const Icon(FluentIcons.settings),
            title: Text(l10n.navSettings),
            body: pages[5],
          ),
          PaneItem(
            icon: const Icon(FluentIcons.info),
            title: Text(l10n.navAbout),
            body: pages[6],
          ),
        ],
      ),
    );
    // 状态栏：Mascots: %1 | Templates: %2（对齐原版 ElaStatusBar）。
    return DropTarget(
      onDragDone: (detail) {
        final paths = detail.files
            .map((f) => f.path)
            .where((p) => p.isNotEmpty)
            .toList();
        _importDropped(paths);
      },
      child: Stack(
        children: [
          Column(
            children: [
              Expanded(child: navigation),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 4,
                ),
                decoration: BoxDecoration(
                  border: Border(
                    top: BorderSide(
                      color: FluentTheme.of(
                        context,
                      ).resources.dividerStrokeColorDefault,
                    ),
                  ),
                ),
                child: Text(
                  key: const Key('manager-status-counts'),
                  l10n.statusBar(state.mascotCount, state.templateCount),
                  style: FluentTheme.of(context).typography.caption,
                ),
              ),
            ],
          ),
          if (_inspectId != null)
            Align(
              alignment: Alignment.centerRight,
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: 420,
                    maxHeight: 520,
                  ),
                  child: Card(
                    child: Padding(
                      padding: const EdgeInsets.all(12),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Row(
                            children: [
                              Expanded(
                                child: Text(
                                  l10n.inspectorTitle(
                                    _inspectName.isEmpty
                                        ? '$_inspectId'
                                        : _inspectName,
                                  ),
                                  style: const TextStyle(
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                              ),
                              IconButton(
                                icon: const Icon(FluentIcons.clear),
                                onPressed: _closeInspector,
                              ),
                            ],
                          ),
                          const SizedBox(height: 8),
                          Expanded(
                            child: SingleChildScrollView(
                              child: SelectableText(
                                _inspectText.isEmpty ? '…' : _inspectText,
                                style: const TextStyle(
                                  fontFamily: 'Consolas',
                                  fontSize: 12,
                                ),
                              ),
                            ),
                          ),
                          const SizedBox(height: 8),
                          Button(
                            onPressed: _closeInspector,
                            child: Text(l10n.inspectorClose),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
