import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

/// Combinations page: save and restore running mascot groups, wired to the
/// runtime save/restore/list/delete_combination commands over POST /command.
class CombinationsPage extends StatefulWidget {
  const CombinationsPage({super.key});

  @override
  State<CombinationsPage> createState() => _CombinationsPageState();
}

class _CombinationsPageState extends State<CombinationsPage> {
  final _nameController = TextEditingController();
  List<String> _combos = [];
  bool _loading = true;
  bool _busy = false;
  String? _error;

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final state = context.read<AppState>();
      await state.refresh();
      final result = await state.api.command({'command': 'list_combinations'});
      if (!mounted) return;
      final list = result['combinations'];
      setState(() {
        _combos = list is List ? list.map((e) => e.toString()).toList() : [];
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

  Future<void> _notify(String title, String message, InfoBarSeverity severity) async {
    await displayInfoBar(context, builder: (context, close) {
      return InfoBar(
        title: Text(title),
        content: Text(message),
        severity: severity,
        action: IconButton(
          icon: const Icon(FluentIcons.clear),
          onPressed: close,
        ),
      );
    });
  }

  Future<void> _save() async {
    final name = _nameController.text.trim();
    if (name.isEmpty) return;
    setState(() => _busy = true);
    try {
      final state = context.read<AppState>();
      final result = await state.api
          .command({'command': 'save_combination', 'name': name});
      if (!mounted) return;
      _nameController.clear();
      await _load();
      await _notify('组合已保存',
          '"${result['name']}"（${result['count']} 只桌宠）', InfoBarSeverity.success);
    } catch (e) {
      if (!mounted) return;
      await _notify('保存失败', e.toString(), InfoBarSeverity.error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _restore(String name) async {
    setState(() => _busy = true);
    try {
      final state = context.read<AppState>();
      final result = await state.api
          .command({'command': 'restore_combination', 'name': name});
      if (!mounted) return;
      await state.refresh();
      await _notify('组合已恢复',
          '"$name"：已召唤 ${result['spawned']} 只桌宠', InfoBarSeverity.success);
    } catch (e) {
      if (!mounted) return;
      await _notify('恢复失败', e.toString(), InfoBarSeverity.error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _delete(String name) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => ContentDialog(
        title: const Text('删除组合'),
        content: Text('确定删除组合 "$name" 吗？此操作不可撤销。'),
        actions: [
          Button(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('删除'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    if (!mounted) return;

    setState(() => _busy = true);
    try {
      final state = context.read<AppState>();
      await state.api.command({'command': 'delete_combination', 'name': name});
      if (!mounted) return;
      await _load();
      await _notify('已删除', '组合 "$name" 已删除', InfoBarSeverity.success);
    } catch (e) {
      if (!mounted) return;
      await _notify('删除失败', e.toString(), InfoBarSeverity.error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final state = context.watch<AppState>();

    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.navCombinations)),
      children: [
        Card(
          margin: const EdgeInsets.all(16),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Row(children: [
              Expanded(
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '当前运行状态',
                        style: FluentTheme.of(context).typography.bodyStrong,
                      ),
                      const SizedBox(height: 4),
                      Text(state.runtimeOnline
                          ? '${state.running.length} 只桌宠正在运行'
                          : '运行时离线，请先在主页启动运行时'),
                    ]),
              ),
              SizedBox(
                width: 220,
                child: TextBox(
                  controller: _nameController,
                  placeholder: '组合名称',
                  onSubmitted: (_) => _save(),
                ),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: (!state.runtimeOnline ||
                        state.running.isEmpty ||
                        _busy)
                    ? null
                    : _save,
                child: Row(mainAxisSize: MainAxisSize.min, children: const [
                  Icon(FluentIcons.save),
                  SizedBox(width: 8),
                  Text('保存组合'),
                ]),
              ),
              const SizedBox(width: 8),
              IconButton(
                icon: const Icon(FluentIcons.refresh),
                onPressed: _loading ? null : _load,
              ),
            ]),
          ),
        ),
        if (_loading)
          const Padding(
            padding: EdgeInsets.all(32),
            child: Center(child: ProgressRing()),
          )
        else if (_error != null)
          Card(
            margin: const EdgeInsets.all(16),
            child: InfoBar(
              title: const Text('无法读取组合列表'),
              content: Text('$_error\n请确认运行时已启动（需要 NEUROLINGSCE_HTTP=1）。'),
              severity: InfoBarSeverity.warning,
            ),
          )
        else if (_combos.isEmpty)
          Card(
            margin: const EdgeInsets.all(16),
            child: Padding(
              padding: const EdgeInsets.all(32),
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                const Icon(FluentIcons.group, size: 48),
                const SizedBox(height: 16),
                Text(
                  '暂无保存的组合',
                  style: FluentTheme.of(context).typography.bodyLarge,
                ),
                const SizedBox(height: 8),
                Text(
                  '先召唤几只桌宠，然后输入名称并点击"保存组合"。\n'
                  '之后可以一键恢复保存的桌宠组；程序退出时也会自动记录最后一次组合。',
                  textAlign: TextAlign.center,
                  style: FluentTheme.of(context).typography.body,
                ),
              ]),
            ),
          )
        else
          ..._combos.map(
            (name) => Card(
              margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
              child: ListTile(
                leading: const Icon(FluentIcons.group),
                title: Text(name),
                trailing: Row(mainAxisSize: MainAxisSize.min, children: [
                  FilledButton(
                    onPressed: _busy ? null : () => _restore(name),
                    child: const Text('恢复'),
                  ),
                  const SizedBox(width: 8),
                  IconButton(
                    icon: const Icon(FluentIcons.delete),
                    onPressed: _busy ? null : () => _delete(name),
                  ),
                ]),
              ),
            ),
          ),
      ],
    );
  }
}
