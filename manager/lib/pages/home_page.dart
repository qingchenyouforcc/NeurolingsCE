import 'dart:io';
import 'dart:math';

import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../api/runtime_api.dart';
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
            _HomeActionBar(state: state),
            const SizedBox(height: 12),
            _RuntimeStatusCard(state: state, l10n: l10n),
            const SizedBox(height: 8),
            _StatusBar(state: state),
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

class _HomeActionBar extends StatelessWidget {
  const _HomeActionBar({required this.state});
  final AppState state;

  @override
  Widget build(BuildContext context) {
    return Wrap(spacing: 8, runSpacing: 8, children: [
      FilledButton(
        onPressed: state.templates.isEmpty ? null : () {
          final rnd = Random();
          final pick = state.templates[rnd.nextInt(state.templates.length)];
          state.spawn(pick.name);
        },
        child: const Text('随机召唤'),
      ),
      Button(
        onPressed: () async {
          // 触发文件选择导入（复用 AppState.importArchive 的 CLI 路径）
          // 简化：提示用户到“创建”页导入
          displayInfoBar(context, builder: (ctx, close) => const InfoBar(title: Text('请到“创建”页导入桌宠包'), severity: InfoBarSeverity.info));
        },
        child: const Text('导入'),
      ),
      Button(
        onPressed: state.busy ? null : () => state.refresh(),
        child: const Text('刷新'),
      ),
      Button(
        onPressed: () async {
          final home = Platform.environment['USERPROFILE'] ?? Platform.environment['HOME'] ?? '';
          final path = Platform.isWindows ? '${Platform.environment['LOCALAPPDATA'] ?? home}\\NeurolingsCE\\mascots' : '$home/.local/share/NeurolingsCE/mascots';
          try {
            if (Platform.isWindows) {
              await Process.run('explorer', [path]);
            } else if (Platform.isMacOS) {
              await Process.run('open', [path]);
            } else {
              await Process.run('xdg-open', [path]);
            }
          } catch (e) {
            if (!context.mounted) return;
            displayInfoBar(context, builder: (ctx, c) => InfoBar(title: Text(e.toString()), severity: InfoBarSeverity.error));
          }
        },
        child: const Text('打开文件夹'),
      ),
    ]);
  }
}

class _StatusBar extends StatelessWidget {
  const _StatusBar({required this.state});
  final AppState state;
  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(color: FluentTheme.of(context).cardColor, borderRadius: BorderRadius.circular(4)),
      child: Row(children: [
        Text('桌宠: ${state.running.length} | 模板: ${state.templates.length}', style: FluentTheme.of(context).typography.caption),
        const Spacer(),
        if (state.lastError != null) Expanded(child: Text(state.lastError!, style: TextStyle(color: Colors.red, fontSize: 12), overflow: TextOverflow.ellipsis)),
      ]),
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
      else if (state.templates.isEmpty)
        Card(child: Column(children: [
          Text('暂无模板', style: FluentTheme.of(context).typography.body),
          const SizedBox(height: 8),
          const Text('到“创建”页导入 .mascot / .zip 包'),
        ]))
      else
        ...state.templates.map(
          (template) => Card(
            margin: const EdgeInsets.symmetric(vertical: 4),
            child: Row(children: [
              // 预览占位 64x64
              Container(width: 48, height: 48, decoration: BoxDecoration(color: Colors.grey[30], borderRadius: BorderRadius.circular(4)), child: const Icon(FluentIcons.photo2, size: 24)),
              const SizedBox(width: 12),
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text(template.name,
                      style: FluentTheme.of(context).typography.bodyStrong),
                  if (template.description.isNotEmpty)
                    Text(template.description,
                        style: FluentTheme.of(context).typography.caption,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis),
                  Text('v${template.version}${template.author.isNotEmpty ? ' · ${template.author}' : ''}', style: FluentTheme.of(context).typography.caption),
                ]),
              ),
              FilledButton(
                onPressed: () => state.spawn(template.name),
                child: Text(l10n.spawn),
              ),
              const SizedBox(width: 8),
              IconButton(
                icon: const Icon(FluentIcons.delete),
                onPressed: () async {
                  final ok = await showDialog<bool>(context: context, builder: (ctx) => ContentDialog(title: const Text('删除模板'), content: Text('确定删除 ${template.name}？'), actions: [Button(onPressed: () => Navigator.pop(ctx, false), child: const Text('取消')), FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('删除'))]));
                  if (ok != true) return;
                  try {
                    final api = RuntimeApi();
                    await api.command({'command': 'remove_mascot_template', 'mascot_name': template.name});
                    if (!context.mounted) return;
                    displayInfoBar(context, builder: (c, close) => InfoBar(title: Text('已删除 ${template.name}'), severity: InfoBarSeverity.success));
                    state.refresh();
                  } catch (e) {
                    if (!context.mounted) return;
                    displayInfoBar(context, builder: (c, close) => InfoBar(title: Text(e.toString()), severity: InfoBarSeverity.error));
                  }
                },
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
