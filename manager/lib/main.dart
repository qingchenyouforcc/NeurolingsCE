import 'dart:io';

import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:provider/provider.dart';
import 'package:window_manager/window_manager.dart';

import 'pages/about_page.dart';
import 'pages/codex_page.dart';
import 'pages/combinations_page.dart';
import 'pages/create_page.dart';
import 'pages/home_page.dart';
import 'pages/inspector_page.dart';
import 'pages/settings_page.dart';
import 'pages/store_page.dart';
import 'state/app_state.dart';
import 'state/settings.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  if (Platform.isWindows || Platform.isLinux || Platform.isMacOS) {
    await windowManager.ensureInitialized();
    const options = WindowOptions(
      size: Size(1000, 680),
      minimumSize: Size(800, 560),
      title: 'NeurolingsCE Manager',
    );
    await windowManager.waitUntilReadyToShow(options, () async {
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
        ChangeNotifierProvider(create: (_) => SettingsController()),
      ],
      child: Consumer<SettingsController>(
        builder: (context, settings, _) {
          return FluentApp(
            title: 'NeurolingsCE Manager',
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

class _ManagerShellState extends State<ManagerShell> {
  int _index = 0;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final pages = <Widget>[
      const HomePage(),
      const CreatePage(),
      const StorePage(),
      const CombinationsPage(),
      const CodexPage(),
      const SettingsPage(),
      const InspectorPage(),
      const AboutPage(),
    ];
      // 主区 5 项 + 底部 3 项，对齐原版 addPageNode vs addFooterNode
      return NavigationView(
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
              body: pages[2]),
          PaneItem(
              icon: const Icon(FluentIcons.fabric_new_folder),
              title: Text(l10n.navCreate),
              body: pages[1]),
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
              icon: const Icon(FluentIcons.view_dashboard),
              title: const Text('检查器'),
              body: pages[6]),
          PaneItem(
              icon: const Icon(FluentIcons.settings),
              title: Text(l10n.navSettings),
              body: pages[5]),
          PaneItem(
              icon: const Icon(FluentIcons.info),
              title: Text(l10n.navAbout),
              body: pages[7]),
        ],
      ),
    );
  }
}
