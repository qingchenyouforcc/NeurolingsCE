import 'dart:io';

import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';
import '../state/settings.dart';

class SettingsPage extends StatelessWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final settings = context.watch<SettingsController>();
    final state = context.watch<AppState>();

    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.navSettings)),
      children: [
        Card(
          child: Row(children: [
            Expanded(
              child: Text(l10n.settingsLanguage,
                  style: FluentTheme.of(context).typography.bodyStrong),
            ),
            ComboBox<String>(
              value: settings.locale,
              items: [
                ComboBoxItem(value: 'en', child: Text('English')),
                ComboBoxItem(value: 'zh', child: Text('中文（简体）')),
              ],
              onChanged: (value) {
                if (value != null) settings.setLocale(value);
              },
            ),
          ]),
        ),
        const SizedBox(height: 8),
        Card(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(l10n.settingsRuntime,
                style: FluentTheme.of(context).typography.bodyStrong),
            const SizedBox(height: 8),
            Text(
              state.runtimeOnline ? l10n.runtimeOnline : l10n.runtimeOffline,
              style: FluentTheme.of(context).typography.caption,
            ),
            const SizedBox(height: 4),
            Text(l10n.settingsHttpHint,
                style: FluentTheme.of(context).typography.caption),
          ]),
        ),
        const SizedBox(height: 8),
        Card(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(l10n.settingsStorage,
                style: FluentTheme.of(context).typography.bodyStrong),
            const SizedBox(height: 8),
            SelectableText(
              storagePathDescription(),
              style: FluentTheme.of(context).typography.caption,
            ),
          ]),
        ),
      ],
    );
  }
}

String storagePathDescription() {
  final home = Platform.environment['USERPROFILE'] ??
      Platform.environment['HOME'] ??
      '';
  if (Platform.isWindows) {
    final local = Platform.environment['LOCALAPPDATA'] ?? home;
    return '$local\\NeurolingsCE\\mascots';
  }
  return '$home/.local/share/NeurolingsCE/mascots';
}
