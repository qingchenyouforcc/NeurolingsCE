import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';

const String appVersion = '0.1.0';

class AboutPage extends StatelessWidget {
  const AboutPage({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.navAbout)),
      children: [
        Card(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text('NeurolingsCE', style: FluentTheme.of(context).typography.title),
            const SizedBox(height: 8),
            Text(l10n.aboutDescription),
            const SizedBox(height: 16),
            Text('${l10n.version}: $appVersion',
                style: FluentTheme.of(context).typography.caption),
            Text('${l10n.license}: GPL-3.0-only',
                style: FluentTheme.of(context).typography.caption),
          ]),
        ),
      ],
    );
  }
}
