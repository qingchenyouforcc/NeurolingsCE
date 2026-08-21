import 'dart:async';

import 'package:fluent_ui/fluent_ui.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

class InspectorPage extends StatefulWidget {
  const InspectorPage({super.key});

  @override
  State<InspectorPage> createState() => _InspectorPageState();
}

class _InspectorPageState extends State<InspectorPage> {
  Map<String, dynamic>? _detail;
  int? _selectedId;
  Timer? _timer;
  bool _auto = true;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (_auto && _selectedId != null) _fetch(_selectedId!);
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    await context.read<AppState>().refresh();
  }

  Future<void> _fetch(int id) async {
    try {
      final state = context.read<AppState>();
      final res = await state.api.command({'command': 'get_mascot', 'mascot_id': id});
      if (!mounted) return;
      setState(() {
        _detail = res['mascot'] is Map ? Map<String, dynamic>.from(res['mascot']) : res;
        _selectedId = id;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _detail = {'error': e.toString()});
    }
  }

  String _vecToString(dynamic v) {
    if (v is Map) {
      final x = v['x'];
      final y = v['y'];
      return '(${x?.toStringAsFixed(1) ?? '?'}, ${y?.toStringAsFixed(1) ?? '?'})';
    }
    return v?.toString() ?? '-';
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<AppState>();
    return ScaffoldPage.scrollable(
      header: PageHeader(
        title: const Text('检查器'),
        commandBar: Row(children: [
          ToggleSwitch(checked: _auto, onChanged: (v) => setState(() => _auto = v)),
          const SizedBox(width: 8),
          const Text('自动刷新'),
          const SizedBox(width: 12),
          Button(onPressed: _load, child: const Text('刷新')),
        ]),
      ),
      children: [
        if (!state.runtimeOnline)
          const InfoBar(title: Text('运行时离线'), content: Text('请先在主页启动运行时'), severity: InfoBarSeverity.warning)
        else ...[
          Card(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('运行中桌宠', style: FluentTheme.of(context).typography.bodyStrong),
              const SizedBox(height: 8),
              if (state.running.isEmpty)
                const Text('暂无运行中的桌宠', style: TextStyle(color: Color(0xFF6B6B6B)))
              else
                Wrap(spacing: 8, runSpacing: 8, children: state.running.map((m) {
                  final selected = m.id == _selectedId;
                  return Button(
                    onPressed: () => _fetch(m.id),
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                      decoration: BoxDecoration(
                        color: selected ? FluentTheme.of(context).accentColor.withValues(alpha: 0.12) : null,
                        border: Border.all(color: selected ? FluentTheme.of(context).accentColor : Colors.grey.withValues(alpha: 0.3)),
                        borderRadius: BorderRadius.circular(6),
                      ),
                      child: Text('#${m.id} ${m.name}'),
                    ),
                  );
                }).toList()),
            ]),
          ),
          const SizedBox(height: 12),
          Card(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('详情', style: FluentTheme.of(context).typography.bodyStrong),
              const SizedBox(height: 8),
              if (_selectedId == null)
                const Text('请选择一个桌宠', style: TextStyle(color: Color(0xFF6B6B6B)))
              else if (_detail == null)
                const Center(child: ProgressRing())
              else if (_detail!.containsKey('error'))
                InfoBar(title: const Text('错误'), content: Text(_detail!['error'].toString()), severity: InfoBarSeverity.error)
              else
                Table(
                  columnWidths: const {0: FixedColumnWidth(160), 1: FlexColumnWidth()},
                  children: [
                    _row('ID', '${_detail!['id'] ?? _selectedId}'),
                    _row('名称', '${_detail!['name'] ?? ''}'),
                    _row('data_id', '${_detail!['data_id'] ?? ''}'),
                    _row('锚点 anchor', _vecToString(_detail!['anchor'])),
                    _row('行为', '${_detail!['active_behavior'] ?? ''}'),
                    _row('标签', '${_detail!['label'] ?? ''}'),
                  ],
                ),
            ]),
          ),
          const SizedBox(height: 12),
          Card(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('原始 JSON', style: FluentTheme.of(context).typography.caption),
              const SizedBox(height: 8),
              SelectableText(_detail?.toString() ?? '—', style: const TextStyle(fontFamily: 'monospace', fontSize: 11)),
            ]),
          ),
        ],
      ],
    );
  }

  TableRow _row(String k, String v) {
    return TableRow(children: [
      Padding(padding: const EdgeInsets.symmetric(vertical: 4), child: Text(k, style: const TextStyle(color: Color(0xFF6B6B6B), fontSize: 12))),
      Padding(padding: const EdgeInsets.symmetric(vertical: 4), child: SelectableText(v, style: const TextStyle(fontSize: 12))),
    ]);
  }
}
