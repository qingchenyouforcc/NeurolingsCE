import 'package:fluent_ui/fluent_ui.dart';
import 'package:intl/intl.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

/// 桌宠组合页：对齐原版 ManagerCombinationsPage.cc 的核心行为
/// - 首项固定为 Last Combination Before Close（不可删）
/// - 详情面板：Saved at / Total / 逐只清单
/// - 保存命名弹窗默认值 Combination %1（本地化短日期）
/// - 安全限位提示 50 / 200
/// - missing / failed 去重报告
/// - 时间本地化
class CombinationsPage extends StatefulWidget {
  const CombinationsPage({super.key});

  @override
  State<CombinationsPage> createState() => _CombinationsPageState();
}

// ---- 常量（与原版 kMaxMascotsPerEntry / kMaxMascotsPerCombination 对齐） ----

const String _kLastBeforeCloseId = 'lastBeforeClose';
const int _kMaxPerEntry = 50;
const int _kMaxPerCombination = 200;

enum _CombinationType { lastBeforeClose, saved }

/// 单个组合的聚合视图（供列表与详情共用）
class _CombinationItem {
  const _CombinationItem({
    required this.id,
    required this.displayName,
    required this.type,
    required this.counts,
    required this.total,
    required this.isRestorable,
    this.savedAt,
  });

  final String id;
  final String displayName;
  final _CombinationType type;
  final Map<String, int> counts;
  final int total;
  final DateTime? savedAt;
  final bool isRestorable;

  bool get isLastBeforeClose => type == _CombinationType.lastBeforeClose;
  bool get canDelete => type == _CombinationType.saved;
}

class _CombinationsPageState extends State<CombinationsPage> {
  bool _loading = true;
  bool _busy = false;
  String? _error;

  List<_CombinationItem> _items = [];
  String? _selectedId;

  bool _isZh(BuildContext context) {
    final locale = Localizations.localeOf(context).languageCode.toLowerCase();
    return locale.startsWith('zh');
  }

  String _t(BuildContext context, String en, String zh) =>
      _isZh(context) ? zh : en;

  /// 本地化短日期（对齐 QLocale::ShortFormat + toLocalTime）
  String _formatShortDate(BuildContext context, DateTime dt) {
    final local = dt.toLocal();
    final isZh = _isZh(context);
    try {
      if (isZh) {
        return DateFormat('yyyy/M/d HH:mm', 'zh_CN').format(local);
      }
      final locale = Localizations.localeOf(context).toString();
      // en: 8/21/2026 7:30 PM  — 与 QLocale ShortFormat 近似
      return DateFormat.yMd(locale).add_jm().format(local);
    } catch (_) {
      return DateFormat('yyyy-MM-dd HH:mm').format(local);
    }
  }

  /// savedAt 为空时显示 Not saved yet / 尚未保存
  String _formatSavedAt(BuildContext context, DateTime? dt) {
    if (dt == null) return _t(context, 'Not saved yet', '尚未保存');
    return _formatShortDate(context, dt);
  }

  /// 默认保存名：Combination %1（本地化日期）
  String _defaultCombinationName(BuildContext context) {
    final now = DateTime.now();
    final formatted = _formatShortDate(context, now);
    if (_isZh(context)) {
      return '组合 $formatted';
    }
    return 'Combination $formatted';
  }

  /// 汇总：与原版 combinationSummary 一致，最多展示 3 项，其余归为 and N more
  String _combinationSummary(BuildContext context, _CombinationItem item) {
    if (item.counts.isEmpty) {
      return _t(context, 'No mascots in this combination.', '此组合没有桌宠。');
    }
    final entries = item.counts.entries.toList()
      ..sort((a, b) => a.key.compareTo(b.key));
    final pieces = <String>[];
    int shown = 0;
    for (final e in entries) {
      if (shown < 3) {
        pieces.add('${e.key} x${e.value}');
      }
      shown++;
    }
    if (shown > 3) {
      pieces.add(_t(context, 'and ${shown - 3} more', '等 ${shown - 3} 个'));
    }
    return pieces.join(', ');
  }

  /// 详情多行文本（对齐原版 combinationDetails）
  String _combinationDetailsText(BuildContext context, _CombinationItem item) {
    final lines = <String>[];
    lines.add(item.displayName);
    lines.add(
      '${_t(context, 'Saved at', '保存时间')}: ${_formatSavedAt(context, item.savedAt)}',
    );
    lines.add('${_t(context, 'Total mascots', '桌宠总数')}: ${item.total}');
    lines.add('');
    if (item.counts.isEmpty) {
      lines.add(_t(context, 'No mascots in this combination.', '此组合没有桌宠。'));
    } else {
      final sorted = item.counts.entries.toList()
        ..sort((a, b) => a.key.compareTo(b.key));
      for (final e in sorted) {
        lines.add('${e.key} x${e.value}');
      }
    }
    return lines.join('\n');
  }

  /// 从后端原始 combination 对象聚合 counts（兼容多种后端形状）
  _CombinationItem _itemFromRaw({
    required String id,
    required String displayName,
    required _CombinationType type,
    Map<String, dynamic>? raw,
  }) {
    Map<String, int> counts = {};
    DateTime? savedAt;
    // 1) 尝试解析 savedAt（ISO8601）
    if (raw != null) {
      final savedAtRaw =
          raw['savedAt'] as String? ?? raw['saved_at'] as String?;
      if (savedAtRaw != null && savedAtRaw.isNotEmpty) {
        try {
          savedAt = DateTime.parse(savedAtRaw);
        } catch (_) {
          savedAt = null;
        }
      }
      // 2) 优先使用 aggregated（新后端）
      final agg = raw['aggregated'];
      if (agg is List && agg.isNotEmpty) {
        for (final v in agg) {
          if (v is Map) {
            final name = (v['name'] as String?)?.trim();
            final count = (v['count'] as num?)?.toInt() ?? 0;
            if (name != null && name.isNotEmpty && count > 0) {
              counts[name] = (counts[name] ?? 0) + count;
            }
          }
        }
      } else {
        // 3) mascots 数组（原版 QJsonArray）
        final mascots = raw['mascots'];
        if (mascots is List) {
          for (final v in mascots) {
            if (v is Map) {
              final name = (v['name'] as String?)?.trim();
              final count = (v['count'] as num?)?.toInt() ?? 0;
              if (name != null && name.isNotEmpty && count > 0) {
                counts[name] = (counts[name] ?? 0) + count;
              }
            }
          }
        } else {
          // 4) members 扁平列表（Rust 当前存储：Vec<CombinationMember>）
          final members = raw['members'];
          if (members is List) {
            for (final v in members) {
              if (v is Map) {
                final name =
                    (v['template'] as String?)?.trim() ??
                    (v['name'] as String?)?.trim();
                if (name != null && name.isNotEmpty) {
                  counts[name] = (counts[name] ?? 0) + 1;
                }
              } else if (v is String && v.trim().isNotEmpty) {
                counts[v.trim()] = (counts[v.trim()] ?? 0) + 1;
              }
            }
          }
        }
      }
    }
    final total = counts.values.fold<int>(0, (a, b) => a + b);
    return _CombinationItem(
      id: id,
      displayName: displayName,
      type: type,
      counts: counts,
      total: total,
      savedAt: savedAt,
      isRestorable: total > 0,
    );
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final state = context.read<AppState>();
      await state.refresh();

      // 拉取组合列表（含关闭前状态）
      final result = await state.api.command({'command': 'list_combinations'});
      if (!mounted) return;

      // 构建 items：首项固定 LastBeforeClose，其余按保存顺序（与原版一致，不排序）
      final items = <_CombinationItem>[];

      final lastDisplay = _t(
        context,
        'Last Combination Before Close',
        '上次关闭前的组合',
      );
      final rawLast = result['last_before_close'];
      _CombinationItem lastItem;
      if (rawLast is Map) {
        lastItem = _itemFromRaw(
          id: _kLastBeforeCloseId,
          displayName: lastDisplay,
          type: _CombinationType.lastBeforeClose,
          raw: rawLast.cast<String, dynamic>(),
        );
      } else {
        lastItem = await _fetchDetailForId(
          _kLastBeforeCloseId,
          lastDisplay,
          _CombinationType.lastBeforeClose,
        );
        if (!mounted) return;
      }
      items.add(lastItem);

      final rawList = result['combinations'];
      if (rawList is List) {
        for (final e in rawList) {
          if (e is! Map) continue;
          final m = e.cast<String, dynamic>();
          final id = (m['id'] as String?) ?? '';
          if (id.isEmpty || id == _kLastBeforeCloseId) continue;
          final name = (m['name'] as String?)?.trim() ?? '';
          final displayName = name.isEmpty
              ? _t(context, 'Untitled Combination', '未命名组合')
              : name;
          items.add(
            _itemFromRaw(
              id: id,
              displayName: displayName,
              type: _CombinationType.saved,
              raw: m,
            ),
          );
        }
      }

      if (!mounted) return;
      setState(() {
        _items = items;
        _loading = false;
        // 保持选中：若原选中仍存在则保留，否则选中首项
        if (_selectedId == null || !_items.any((e) => e.id == _selectedId)) {
          _selectedId = _items.isNotEmpty ? _items.first.id : null;
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  /// 拉取单个组合的聚合详情（失败则返回空聚合）
  Future<_CombinationItem> _fetchDetailForId(
    String id,
    String displayName,
    _CombinationType type,
  ) async {
    try {
      final state = context.read<AppState>();
      final res = await state.api.command({
        'command': 'get_combination',
        'id': id,
      });
      if (res.containsKey('error')) {
        // 后端未实现或无此组合
        if (res['code'] == 'combination_not_found' || res['status'] == 404) {
          return _itemFromRaw(
            id: id,
            displayName: displayName,
            type: type,
            raw: null,
          );
        }
        // 其他错误也视为空
        return _itemFromRaw(
          id: id,
          displayName: displayName,
          type: type,
          raw: null,
        );
      }
      // 兼容多种包装：{combination: {...}} 或直接 {...}
      Map<String, dynamic>? payload;
      if (res['combination'] is Map) {
        payload = (res['combination'] as Map).cast<String, dynamic>();
      } else if (res['members'] is List ||
          res['mascots'] is List ||
          res['aggregated'] is List) {
        payload = res;
      } else if (res['name'] != null) {
        payload = res;
      }
      return _itemFromRaw(
        id: id,
        displayName: displayName,
        type: type,
        raw: payload,
      );
    } catch (_) {
      return _itemFromRaw(
        id: id,
        displayName: displayName,
        type: type,
        raw: null,
      );
    }
  }

  Future<void> _notify(
    String title,
    String message,
    InfoBarSeverity severity,
  ) async {
    if (!mounted) return;
    await displayInfoBar(
      context,
      builder: (context, close) {
        return InfoBar(
          title: Text(title),
          content: Text(message),
          severity: severity,
          action: IconButton(
            icon: const Icon(FluentIcons.clear),
            onPressed: close,
          ),
        );
      },
    );
  }

  /// 保存当前运行组合：弹窗默认值 Combination %1，支持空名回退
  Future<void> _saveCurrent() async {
    final state = context.read<AppState>();
    if (!state.runtimeOnline) {
      await _notify(
        _t(context, 'Runtime offline', '运行时离线'),
        _t(context, 'Start the runtime on the Home page first.', '请先在主页启动运行时。'),
        InfoBarSeverity.warning,
      );
      return;
    }
    if (state.running.isEmpty) {
      await _notify(
        _t(context, 'No mascots to save', '没有可保存的桌宠'),
        _t(
          context,
          'There are no active mascots to save.',
          '当前没有运行中的桌宠，无法保存组合。',
        ),
        InfoBarSeverity.warning,
      );
      return;
    }

    final defaultName = _defaultCombinationName(context);
    final controller = TextEditingController(text: defaultName);
    // 选中全部，方便直接改名
    controller.selection = TextSelection(
      baseOffset: 0,
      extentOffset: controller.text.length,
    );

    final confirmedName = await showDialog<String>(
      context: context,
      builder: (dialogContext) => ContentDialog(
        title: Text(_t(context, 'Save Combination', '保存组合')),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(_t(context, 'Combination name:', '组合名称：')),
            const SizedBox(height: 12),
            TextBox(
              controller: controller,
              autofocus: true,
              placeholder: defaultName,
              onSubmitted: (v) => Navigator.pop(dialogContext, v),
            ),
            const SizedBox(height: 12),
            Text(
              _t(
                context,
                'Limit: at most $_kMaxPerEntry per mascot, $_kMaxPerCombination per combination. Excess will be clamped on restore.',
                '限位：单种桌宠最多 $_kMaxPerEntry 只，单组合最多 $_kMaxPerCombination 只，超出部分会在恢复时被截断。',
              ),
              style: FluentTheme.of(context).typography.caption,
            ),
          ],
        ),
        actions: [
          Button(
            onPressed: () => Navigator.pop(dialogContext, null),
            child: Text(_t(context, 'Cancel', '取消')),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, controller.text),
            child: Text(_t(context, 'Save', '保存')),
          ),
        ],
      ),
    );

    controller.dispose();
    if (confirmedName == null) return; // 取消
    var name = confirmedName.trim();
    if (name.isEmpty) name = defaultName;

    setState(() => _busy = true);
    try {
      final result = await state.api.command({
        'command': 'save_combination',
        'name': name,
      });
      if (!mounted) return;
      await _load();
      if (!mounted) return;
      final count = result['count'];
      await _notify(
        _t(context, 'Combination saved', '组合已保存'),
        '"$name" — ${count is int ? count : state.running.length} ${_t(context, 'mascots', '只桌宠')}',
        InfoBarSeverity.success,
      );
    } catch (e) {
      if (!mounted) return;
      await _notify(
        _t(context, 'Save failed', '保存失败'),
        e.toString(),
        InfoBarSeverity.error,
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// 恢复组合：处理 missing / failed 去重报告与限位提示
  Future<void> _restore(_CombinationItem item) async {
    if (!item.isRestorable) {
      await _notify(
        _t(context, 'Nothing to restore', '无可恢复内容'),
        _t(
          context,
          'This combination does not contain any mascots.',
          '此组合不包含任何桌宠。',
        ),
        InfoBarSeverity.warning,
      );
      return;
    }
    setState(() => _busy = true);
    try {
      final state = context.read<AppState>();
      final result = await state.api.command({
        'command': 'restore_combination',
        'id': item.id,
      });
      if (!mounted) return;
      await state.refresh();
      if (!mounted) return;

      final spawned = (result['spawned'] as num?)?.toInt() ?? item.total;
      final missingRaw = result['missing'];
      final failedRaw = result['failed'];
      List<String> missing = [];
      List<String> failed = [];
      if (missingRaw is List) {
        missing = missingRaw
            .map((e) => e.toString())
            .where((s) => s.isNotEmpty)
            .toList();
      }
      if (failedRaw is List) {
        failed = failedRaw
            .map((e) => e.toString())
            .where((s) => s.isNotEmpty)
            .toList();
      }
      // 去重（与原版 missing.removeDuplicates / failed.removeDuplicates 对齐）
      missing = missing.toSet().toList();
      failed = failed.toSet().toList();

      // 限位提示：若总数接近或超过200，追加提示
      final atLimit =
          spawned >= _kMaxPerCombination || item.total > _kMaxPerCombination;

      if (missing.isNotEmpty) {
        await _notify(
          _t(context, 'Restored with missing templates', '已恢复（部分模板缺失）'),
          _t(
            context,
            'Restored $spawned mascot(s). Missing templates: ${missing.join(', ')}',
            '已恢复 $spawned 只桌宠。缺失模板：${missing.join('、')}',
          ),
          InfoBarSeverity.warning,
        );
      } else if (failed.isNotEmpty) {
        await _notify(
          _t(context, 'Restored with failures', '已恢复（部分启动失败）'),
          _t(
            context,
            'Restored $spawned mascot(s). Some mascots could not be started: ${failed.join(', ')}',
            '已恢复 $spawned 只桌宠。部分桌宠未能启动：${failed.join('、')}',
          ),
          InfoBarSeverity.warning,
        );
      } else {
        var msg =
            '"${item.displayName}" — ${_t(context, 'spawned $spawned mascot(s)', '已召唤 $spawned 只桌宠')}';
        if (atLimit) {
          msg +=
              '  (${_t(context, 'Safety limit: $_kMaxPerCombination per combination', '安全限位：单组合最多 $_kMaxPerCombination 只')})';
        }
        await _notify(
          _t(context, 'Combination restored', '组合已恢复'),
          msg,
          InfoBarSeverity.success,
        );
      }
    } catch (e) {
      if (!mounted) return;
      await _notify(
        _t(context, 'Restore failed', '恢复失败'),
        e.toString(),
        InfoBarSeverity.error,
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _delete(_CombinationItem item) async {
    if (!item.canDelete) return;
    final isZh = _isZh(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => ContentDialog(
        title: Text(isZh ? '删除组合' : 'Delete Combination'),
        content: Text(
          isZh
              ? '确定删除组合 "${item.displayName}" 吗？此操作不可撤销。'
              : 'Delete saved combination "${item.displayName}"? This cannot be undone.',
        ),
        actions: [
          Button(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: Text(isZh ? '取消' : 'Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: Text(isZh ? '删除' : 'Delete'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    if (!mounted) return;
    setState(() => _busy = true);
    try {
      final state = context.read<AppState>();
      await state.api.command({'command': 'delete_combination', 'id': item.id});
      if (!mounted) return;
      // 删除后选中回到首项
      _selectedId = _kLastBeforeCloseId;
      await _load();
      if (!mounted) return;
      await _notify(
        _t(context, 'Deleted', '已删除'),
        _t(
          context,
          'Combination "${item.displayName}" deleted.',
          '组合 "${item.displayName}" 已删除。',
        ),
        InfoBarSeverity.success,
      );
    } catch (e) {
      if (!mounted) return;
      await _notify(
        _t(context, 'Delete failed', '删除失败'),
        e.toString(),
        InfoBarSeverity.error,
      );
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
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final state = context.watch<AppState>();
    final isZh = _isZh(context);

    // 选中项
    _CombinationItem? selected;
    if (_selectedId != null) {
      try {
        selected = _items.firstWhere((e) => e.id == _selectedId);
      } catch (_) {
        selected = null;
      }
    }

    return ScaffoldPage.scrollable(
      header: PageHeader(
        title: Text(l10n.navCombinations),
        commandBar: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Button(
              onPressed: _loading || _busy ? null : _load,
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(FluentIcons.refresh),
                  const SizedBox(width: 8),
                  Text(_t(context, 'Refresh', '刷新')),
                ],
              ),
            ),
          ],
        ),
      ),
      children: [
        // 顶部说明
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Text(
            isZh
                ? '保存当前桌面的桌宠组合，稍后一键恢复相同阵容。程序关闭时也会自动记录“上次关闭前的组合”。'
                : 'Save the mascots currently on your desktop and restore the same mix later.',
            style: FluentTheme.of(context).typography.caption,
          ),
        ),
        const SizedBox(height: 12),

        // 操作面板：Save / Refresh（响应式）
        Card(
          margin: const EdgeInsets.symmetric(horizontal: 16),
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: LayoutBuilder(
              builder: (context, constraints) {
                final compact = constraints.maxWidth < 520;
                final saveButton = FilledButton(
                  onPressed: (_busy || !state.runtimeOnline)
                      ? null
                      : _saveCurrent,
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Icon(FluentIcons.save),
                      const SizedBox(width: 8),
                      Text(isZh ? '保存当前组合' : 'Save Current Combination'),
                    ],
                  ),
                );
                final refreshButton = Button(
                  onPressed: _loading ? null : _load,
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Icon(FluentIcons.refresh),
                      const SizedBox(width: 6),
                      Text(isZh ? '刷新' : 'Refresh'),
                    ],
                  ),
                );
                if (compact) {
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      saveButton,
                      const SizedBox(height: 8),
                      refreshButton,
                    ],
                  );
                }
                return Row(
                  children: [
                    saveButton,
                    const SizedBox(width: 8),
                    refreshButton,
                    const Spacer(),
                    Text(
                      isZh
                          ? '${state.running.length} 只桌宠正在运行'
                          : '${state.running.length} mascots running',
                      style: FluentTheme.of(context).typography.caption,
                    ),
                  ],
                );
              },
            ),
          ),
        ),
        const SizedBox(height: 8),

        // 安全限位提示 50/200
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: InfoBar(
            title: Text(isZh ? '安全限位' : 'Safety limits'),
            content: Text(
              isZh
                  ? '单种桌宠最多 $_kMaxPerEntry 只，单组合最多 $_kMaxPerCombination 只。恢复时超出部分将被截断并在日志中警告。'
                  : 'At most $_kMaxPerEntry per mascot and $_kMaxPerCombination per combination. Excess is clamped on restore.',
            ),
            severity: InfoBarSeverity.info,
            isLong: true,
          ),
        ),
        const SizedBox(height: 8),

        const SizedBox(height: 8),

        if (_loading)
          const Padding(
            padding: EdgeInsets.all(32),
            child: Center(child: ProgressRing()),
          )
        else if (_error != null)
          Card(
            margin: const EdgeInsets.symmetric(horizontal: 16),
            child: InfoBar(
              title: Text(isZh ? '无法读取组合列表' : 'Failed to load combinations'),
              content: Text(
                '$_error\n${AppLocalizations.of(context).combinationsOfflineHint}',
              ),
              severity: InfoBarSeverity.warning,
              isLong: true,
            ),
          )
        else
          // 列表 + 详情 响应式布局
          LayoutBuilder(
            builder: (context, constraints) {
              final isCompact = constraints.maxWidth < 720;
              final listPanel = Card(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        isZh ? '已保存的组合' : 'Saved Combinations',
                        style: const TextStyle(fontWeight: FontWeight.w600),
                      ),
                      const SizedBox(height: 8),
                      // 首项 Last Before Close 固定行始终展示
                      ..._items.map((item) {
                        final isSelected = item.id == _selectedId;
                        final summary = _combinationSummary(context, item);
                        final total = item.total;
                        return Padding(
                          padding: const EdgeInsets.symmetric(vertical: 4),
                          child: HoverButton(
                            onPressed: () =>
                                setState(() => _selectedId = item.id),
                            builder: (context, states) => Container(
                              decoration: BoxDecoration(
                                color: isSelected
                                    ? FluentTheme.of(
                                        context,
                                      ).accentColor.withValues(alpha: 0.12)
                                    : (states.isHovered
                                          ? FluentTheme.of(
                                              context,
                                            ).resources.controlFillColorDefault
                                          : Colors.transparent),
                                border: Border.all(
                                  color: isSelected
                                      ? FluentTheme.of(context).accentColor
                                      : Colors.grey.withValues(alpha: 0.3),
                                  width: isSelected ? 1.4 : 0.6,
                                ),
                                borderRadius: BorderRadius.circular(7),
                              ),
                              padding: const EdgeInsets.symmetric(
                                horizontal: 10,
                                vertical: 10,
                              ),
                              child: Row(
                                children: [
                                  Icon(
                                    item.isLastBeforeClose
                                        ? FluentIcons.history
                                        : FluentIcons.group,
                                    size: 16,
                                    color: item.isRestorable
                                        ? null
                                        : FluentTheme.of(
                                            context,
                                          ).resources.textFillColorDisabled,
                                  ),
                                  const SizedBox(width: 10),
                                  Expanded(
                                    child: Column(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.start,
                                      children: [
                                        Text(
                                          item.displayName,
                                          style: FluentTheme.of(context)
                                              .typography
                                              .bodyStrong
                                              ?.copyWith(
                                                color: item.isRestorable
                                                    ? null
                                                    : FluentTheme.of(context)
                                                          .resources
                                                          .textFillColorDisabled,
                                              ),
                                        ),
                                        const SizedBox(height: 2),
                                        Text(
                                          total == 0
                                              ? (isZh ? '空组合' : 'Empty')
                                              : '$summary  ·  $total ${isZh
                                                    ? '只'
                                                    : total == 1
                                                    ? 'mascot'
                                                    : 'mascots'}',
                                          style: FluentTheme.of(context)
                                              .typography
                                              .caption
                                              ?.copyWith(
                                                color: FluentTheme.of(context)
                                                    .resources
                                                    .textFillColorSecondary,
                                              ),
                                          maxLines: 2,
                                          overflow: TextOverflow.ellipsis,
                                        ),
                                      ],
                                    ),
                                  ),
                                  if (item.isLastBeforeClose)
                                    Container(
                                      padding: const EdgeInsets.symmetric(
                                        horizontal: 6,
                                        vertical: 2,
                                      ),
                                      decoration: BoxDecoration(
                                        color: FluentTheme.of(context)
                                            .resources
                                            .cardBackgroundFillColorDefault,
                                        borderRadius: BorderRadius.circular(4),
                                        border: Border.all(
                                          color: Colors.grey.withValues(
                                            alpha: 0.3,
                                          ),
                                        ),
                                      ),
                                      child: Text(
                                        isZh ? '自动' : 'Auto',
                                        style: FluentTheme.of(
                                          context,
                                        ).typography.caption,
                                      ),
                                    ),
                                ],
                              ),
                            ),
                          ),
                        );
                      }),
                      if (_items.length == 1 && _items.first.total == 0)
                        Padding(
                          padding: const EdgeInsets.only(top: 12),
                          child: Text(
                            isZh
                                ? '暂无保存的组合。先召唤几只桌宠，然后点击“保存当前组合”。'
                                : 'No saved combinations yet. Summon a few mascots and save the current mix.',
                            style: FluentTheme.of(context).typography.caption,
                          ),
                        ),
                    ],
                  ),
                ),
              );

              final detailsPanel = Card(
                child: Padding(
                  padding: const EdgeInsets.all(14),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        isZh ? '详情' : 'Details',
                        style: const TextStyle(fontWeight: FontWeight.w600),
                      ),
                      const SizedBox(height: 8),
                      if (selected == null)
                        Text(
                          isZh ? '请选择一个组合。' : 'Select a combination.',
                          style: FluentTheme.of(context).typography.body
                              ?.copyWith(
                                color: FluentTheme.of(
                                  context,
                                ).resources.textFillColorSecondary,
                              ),
                        )
                      else ...[
                        SelectableText(
                          _combinationDetailsText(context, selected),
                          style: FluentTheme.of(context).typography.body,
                        ),
                        const SizedBox(height: 16),
                        // 操作按钮
                        LayoutBuilder(
                          builder: (context, c2) {
                            final compactBtn = c2.maxWidth < 520;
                            final restoreBtn = FilledButton(
                              onPressed:
                                  (_busy ||
                                      selected == null ||
                                      !selected.isRestorable)
                                  ? null
                                  : () => _restore(selected!),
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  const Icon(FluentIcons.forward, size: 14),
                                  const SizedBox(width: 6),
                                  Text(isZh ? '恢复组合' : 'Restore Combination'),
                                ],
                              ),
                            );
                            final deleteBtn = Button(
                              onPressed:
                                  (_busy ||
                                      selected == null ||
                                      !selected.canDelete)
                                  ? null
                                  : () => _delete(selected!),
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  const Icon(FluentIcons.delete, size: 14),
                                  const SizedBox(width: 6),
                                  Text(
                                    isZh
                                        ? '删除已保存组合'
                                        : 'Delete Saved Combination',
                                  ),
                                ],
                              ),
                            );
                            if (compactBtn) {
                              return Column(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  restoreBtn,
                                  const SizedBox(height: 8),
                                  deleteBtn,
                                ],
                              );
                            }
                            return Row(
                              children: [
                                restoreBtn,
                                const SizedBox(width: 8),
                                deleteBtn,
                              ],
                            );
                          },
                        ),
                        if (!selected.isRestorable)
                          Padding(
                            padding: const EdgeInsets.only(top: 8),
                            child: Text(
                              isZh
                                  ? '此组合为空，无法恢复。'
                                  : 'This combination is empty and cannot be restored.',
                              style: FluentTheme.of(context).typography.caption
                                  ?.copyWith(
                                    color: FluentTheme.of(
                                      context,
                                    ).resources.textFillColorSecondary,
                                  ),
                            ),
                          ),
                        if (selected.canDelete == false)
                          Padding(
                            padding: const EdgeInsets.only(top: 4),
                            child: Text(
                              isZh
                                  ? '“上次关闭前的组合”为系统自动保存，不可删除。'
                                  : '"Last Combination Before Close" is auto-saved and cannot be deleted.',
                              style: FluentTheme.of(context).typography.caption
                                  ?.copyWith(
                                    color: FluentTheme.of(
                                      context,
                                    ).resources.textFillColorSecondary,
                                  ),
                            ),
                          ),
                      ],
                    ],
                  ),
                ),
              );

              if (isCompact) {
                return Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: Column(
                    children: [
                      listPanel,
                      const SizedBox(height: 8),
                      detailsPanel,
                    ],
                  ),
                );
              }
              return Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(flex: 2, child: listPanel),
                    const SizedBox(width: 10),
                    Expanded(child: detailsPanel),
                  ],
                ),
              );
            },
          ),

        // 底部运行时离线提示
        if (!state.runtimeOnline && !_loading)
          Padding(
            padding: const EdgeInsets.all(16),
            child: InfoBar(
              title: Text(isZh ? '运行时离线' : 'Runtime offline'),
              content: Text(
                isZh ? '请先在主页启动运行时。' : 'Start the runtime on the Home page.',
              ),
              severity: InfoBarSeverity.warning,
            ),
          ),
      ],
    );
  }
}
