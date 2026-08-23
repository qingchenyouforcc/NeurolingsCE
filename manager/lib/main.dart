import 'dart:async';
import 'dart:io';

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
          create: (_) => SettingsController()),
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

  @override
  void initState() {
    super.initState();
    windowManager.addListener(this);
    windowManager.setPreventClose(true);
    // 定期上报窗口矩形：召唤落点跟随管理器所在屏（对齐原版语义）；
    // 响应携带"跳转 Codex 页"请求（点击 Codex 气泡触发）。
    _heartbeat = Timer.periodic(const Duration(seconds: 1), (_) => _reportWindowRect());
    // 启动即确保运行时在线（对齐原版"Manager 即运行时"的体感）。
    WidgetsBinding.instance.addPostFrameCallback((_) => _ensureRuntime());
  }

  @override
  void dispose() {
    _heartbeat?.cancel();
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
      final response = await state.api.command({
        'command': 'manager_heartbeat',
        'x': position.dx.round(),
        'y': position.dy.round(),
        'width': size.width.round(),
        'height': size.height.round(),
      });
      if (response['codex_navigate'] == true && mounted) {
        // 点击 Codex 气泡：跳转 Codex 页（索引 4，对齐原版 showCodexPage）。
        setState(() => _index = 4);
      }
      if (response['update_navigate'] == true && mounted) {
        // 启动检查发现新版本：跳转 About 页（对齐原版托盘通知点击行为）。
        setState(() => _index = 6);
      }
    } catch (_) {
      // 运行时离线时静默跳过。
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
              body: pages[0]),
          PaneItem(
              icon: const Icon(FluentIcons.shop),
              title: Text(l10n.navStore),
              body: pages[1]),
          PaneItem(
              icon: const Icon(FluentIcons.fabric_new_folder),
              title: Text(l10n.navCreate),
              body: pages[2]),
          PaneItem(
              icon: const Icon(FluentIcons.group),
              title: Text(l10n.navCombinations),
              body: pages[3]),
          PaneItem(
              icon: const Icon(FluentIcons.robot),
              title: Text(l10n.navCodex),
              body: pages[4]),
        ],
        footerItems: [
          PaneItem(
              icon: const Icon(FluentIcons.settings),
              title: Text(l10n.navSettings),
              body: pages[5]),
          PaneItem(
              icon: const Icon(FluentIcons.info),
              title: Text(l10n.navAbout),
              body: pages[6]),
        ],
      ),
    );
    // 状态栏：Mascots: %1 | Templates: %2（对齐原版 ElaStatusBar）。
    return Column(children: [
      Expanded(child: navigation),
      Container(
        width: double.infinity,
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
        decoration: BoxDecoration(
          border: Border(
              top: BorderSide(
                  color:
                      FluentTheme.of(context).resources.dividerStrokeColorDefault)),
        ),
        child: Text(
          '  Mascots: ${state.running.length}  |  Templates: ${state.templates.length}',
          style: FluentTheme.of(context).typography.caption,
        ),
      ),
    ]);
  }
}
