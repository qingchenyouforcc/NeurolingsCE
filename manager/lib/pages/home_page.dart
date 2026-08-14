import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

/// Home: runtime status, installed templates (spawn), running mascots (dismiss).
class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      context.read<AppState>().refresh();
    });
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return Consumer<AppState>(
      builder: (context, state, _) {
        return ScaffoldPage.scrollable(
          header: PageHeader(title: Text(l10n.navHome)),
          children: [
            _RuntimeStatusCard(state: state, l10n: l10n),
            const SizedBox(height: 16),
            _InstalledSection(state: state, l10n: l10n),
            const SizedBox(height: 24),
            _RunningSection(state: state, l10n: l10n),
          ],
        );
      },
    );
  }
}

class _RuntimeStatusCard extends StatelessWidget {
  const _RuntimeStatusCard({required this.state, required this.l10n});

  final AppState state;
  final AppLocalizations l10n;

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Row(children: [
        Icon(
          state.runtimeOnline ? FluentIcons.check_mark : FluentIcons.warning,
          color: state.runtimeOnline ? Colors.green : Colors.orange,
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Text(
            state.runtimeOnline ? l10n.runtimeOnline : l10n.runtimeOffline,
            style: FluentTheme.of(context).typography.bodyStrong,
          ),
        ),
        if (!state.runtimeOnline)
          FilledButton(
            onPressed: state.busy ? null : () => state.startRuntime(),
            child: Text(l10n.startRuntime),
          ),
        IconButton(
          icon: const Icon(FluentIcons.refresh),
          onPressed: state.busy ? null : () => state.refresh(),
        ),
      ]),
    );
  }
}

class _InstalledSection extends StatelessWidget {
  const _InstalledSection({required this.state, required this.l10n});

  final AppState state;
  final AppLocalizations l10n;

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(l10n.loadedMascots, style: FluentTheme.of(context).typography.subtitle),
      const SizedBox(height: 8),
      if (!state.runtimeOnline)
        Text(l10n.runtimeOffline, style: FluentTheme.of(context).typography.caption)
      else if (state.templates.isEmpty)
        Text(l10n.noTemplates, style: FluentTheme.of(context).typography.caption)
      else
        ...state.templates.map(
          (template) => Card(
            margin: const EdgeInsets.symmetric(vertical: 4),
            child: Row(children: [
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text(template.name,
                      style: FluentTheme.of(context).typography.bodyStrong),
                  if (template.description.isNotEmpty)
                    Text(template.description,
                        style: FluentTheme.of(context).typography.caption,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis),
                ]),
              ),
              if (template.version.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  child: Text('v${template.version}',
                      style: FluentTheme.of(context).typography.caption),
                ),
              FilledButton(
                onPressed: () => state.spawn(template.name),
                child: Text(l10n.spawn),
              ),
            ]),
          ),
        ),
    ]);
  }
}

class _RunningSection extends StatelessWidget {
  const _RunningSection({required this.state, required this.l10n});

  final AppState state;
  final AppLocalizations l10n;

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Row(children: [
        Expanded(
          child: Text(l10n.runningMascots,
              style: FluentTheme.of(context).typography.subtitle),
        ),
        if (state.running.isNotEmpty)
          Button(
            onPressed: () => state.dismissAll(),
            child: Text(l10n.dismissAll),
          ),
      ]),
      const SizedBox(height: 8),
      if (!state.runtimeOnline)
        Text(l10n.runtimeOffline, style: FluentTheme.of(context).typography.caption)
      else if (state.running.isEmpty)
        Text(l10n.noRunning, style: FluentTheme.of(context).typography.caption)
      else
        ...state.running.map(
          (mascot) => Card(
            margin: const EdgeInsets.symmetric(vertical: 4),
            child: Row(children: [
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text('#${mascot.id} ${mascot.name}',
                      style: FluentTheme.of(context).typography.bodyStrong),
                  Text(
                    mascot.activeBehavior ?? '',
                    style: FluentTheme.of(context).typography.caption,
                  ),
                ]),
              ),
              Button(
                onPressed: () => state.dismiss(mascot.id),
                child: Text(l10n.dismiss),
              ),
            ]),
          ),
        ),
    ]);
  }
}
