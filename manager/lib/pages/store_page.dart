import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

/// Store page: browse and install mascots from the configured registry,
/// wired to the runtime store_status / store_index / store_install commands.
class StorePage extends StatefulWidget {
  const StorePage({super.key});

  @override
  State<StorePage> createState() => _StorePageState();
}

class _StorePageEntry {
  final String id;
  final String name;
  final String version;
  final String summary;
  final String license;
  final List<String> authors;
  final int size;
  final String iconUrl;

  _StorePageEntry.fromJson(Map<String, dynamic> json)
      : id = (json['id'] as String?) ?? '',
        name = (json['name'] as String?) ?? '',
        version = (json['version'] as String?) ?? '',
        summary = (json['summary'] as String?) ?? '',
        license = (json['license'] as String?) ?? '',
        authors = (json['authors'] as List?)?.map((e) => e.toString()).toList() ?? [],
        size = ((json['download'] as Map?)?['size'] as num?)?.toInt() ?? -1,
        iconUrl = ((json['icon'] as Map?)?['url'] as String?) ?? '';
}

class _StorePageState extends State<StorePage> {
  bool _loading = false;
  bool _installing = false;
  bool _configured = false;
  String _indexUrl = '';
  List<_StorePageEntry> _entries = [];
  String? _error;
  String _search = '';

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
  }

  Future<void> _load({bool refresh = false}) async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final state = context.read<AppState>();
      if (!state.runtimeOnline) {
        await state.refresh();
      }
      final status = await state.api.command({'command': 'store_status'});
      final configured = status['configured'] == true;
      final url = (status['index_url'] as String?) ?? '';

      if (!configured) {
        if (!mounted) return;
        setState(() {
          _configured = false;
          _indexUrl = url;
          _entries = [];
          _loading = false;
        });
        return;
      }

      final result = await state.api
          .command({'command': 'store_index', 'refresh': refresh});
      if (!mounted) return;
      final list = result['entries'];
      setState(() {
        _configured = true;
        _indexUrl = url;
        _entries = list is List
            ? list
                .whereType<Map<String, dynamic>>()
                .map(_StorePageEntry.fromJson)
                .toList()
            : [];
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

  Future<void> _install(_StorePageEntry entry) async {
    setState(() => _installing = true);
    try {
      final state = context.read<AppState>();
      final result =
          await state.api.command({'command': 'store_install', 'id': entry.id});
      if (!mounted) return;
      await state.refresh();
      if (!mounted) return;
      final entryInfo = result['store_entry'];
      final name = entryInfo is Map ? (entryInfo['name'] ?? entry.name) : entry.name;
      await displayInfoBar(context, builder: (context, close) {
        return InfoBar(
          title: Text('安装成功：$name'),
          content: const Text('已通过 SHA-256 校验并导入模板库，可在主页召唤。'),
          severity: InfoBarSeverity.success,
          action: IconButton(
            icon: const Icon(FluentIcons.clear),
            onPressed: close,
          ),
        );
      });
    } catch (e) {
      if (!mounted) return;
      await displayInfoBar(context, builder: (context, close) {
        return InfoBar(
          title: Text('安装失败：${entry.name}'),
          content: Text(e.toString()),
          severity: InfoBarSeverity.error,
          action: IconButton(
            icon: const Icon(FluentIcons.clear),
            onPressed: close,
          ),
        );
      });
    } finally {
      if (mounted) setState(() => _installing = false);
    }
  }

  String _formatSize(int bytes) {
    if (bytes < 0) return '';
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
    return '${(bytes / 1024 / 1024).toStringAsFixed(1)} MB';
  }

  List<_StorePageEntry> get _filtered {
    final query = _search.trim().toLowerCase();
    if (query.isEmpty) return _entries;
    return _entries.where((e) {
      return e.name.toLowerCase().contains(query) ||
          e.summary.toLowerCase().contains(query) ||
          e.authors.any((a) => a.toLowerCase().contains(query));
    }).toList();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final state = context.watch<AppState>();

    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.navStore)),
      children: [
        Card(
          margin: const EdgeInsets.all(16),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Row(children: [
              const Icon(FluentIcons.shop, size: 24),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '商店索引',
                        style: FluentTheme.of(context).typography.bodyStrong,
                      ),
                      const SizedBox(height: 4),
                      SelectableText(
                        _indexUrl.isEmpty ? '（未配置）' : _indexUrl,
                        style: FluentTheme.of(context).typography.caption,
                      ),
                    ]),
              ),
              IconButton(
                icon: const Icon(FluentIcons.refresh),
                onPressed: _loading ? null : () => _load(refresh: true),
              ),
            ]),
          ),
        ),
        if (_loading)
          const Padding(
            padding: EdgeInsets.all(32),
            child: Center(child: ProgressRing()),
          )
        else if (!state.runtimeOnline)
          Card(
            margin: const EdgeInsets.all(16),
            child: InfoBar(
              title: const Text('运行时离线'),
              content: const Text('请先在主页启动运行时，再浏览商店。'),
              severity: InfoBarSeverity.warning,
            ),
          )
        else if (!_configured)
          Card(
            margin: const EdgeInsets.all(16),
            child: Padding(
              padding: const EdgeInsets.all(32),
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                const Icon(FluentIcons.shop, size: 48),
                const SizedBox(height: 16),
                Text(
                  '商店未配置',
                  style: FluentTheme.of(context).typography.bodyLarge,
                ),
                const SizedBox(height: 8),
                const Text(
                  '设置环境变量 NEUROLINGSCE_MASCOT_INDEX_URL 指向商店索引\n'
                  '（例如 https://blog.qingchenyou.asia/NeurolingsCE-Mascots-Staging/index-v1.json）\n'
                  '然后重启运行时即可浏览并安装官方桌宠包。',
                  textAlign: TextAlign.center,
                ),
              ]),
            ),
          )
        else if (_error != null)
          Card(
            margin: const EdgeInsets.all(16),
            child: InfoBar(
              title: const Text('加载索引失败'),
              content: Text(_error!),
              severity: InfoBarSeverity.error,
            ),
          )
        else ...[
          if (_entries.isNotEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: TextBox(
                placeholder: '搜索名称、简介或作者...',
                onChanged: (value) => setState(() => _search = value),
              ),
            ),
          if (_filtered.isEmpty)
            Card(
              margin: const EdgeInsets.all(16),
              child: Padding(
                padding: const EdgeInsets.all(32),
                child: Column(mainAxisSize: MainAxisSize.min, children: [
                  const Icon(FluentIcons.search, size: 48),
                  const SizedBox(height: 16),
                  Text(
                    _entries.isEmpty ? '商店暂无桌宠包' : '没有匹配的桌宠',
                    style: FluentTheme.of(context).typography.bodyLarge,
                  ),
                ]),
              ),
            )
          else
            ..._filtered.map(
              (entry) => Card(
                margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                child: ListTile(
                  leading: entry.iconUrl.isNotEmpty
                      ? ClipRRect(
                          borderRadius: BorderRadius.circular(4),
                          child: Image.network(
                            entry.iconUrl,
                            width: 40,
                            height: 40,
                            fit: BoxFit.cover,
                            errorBuilder: (context, error, stack) =>
                                const Icon(FluentIcons.unknown),
                          ),
                        )
                      : const Icon(FluentIcons.unknown, size: 40),
                  title: Text(entry.name,
                      style: FluentTheme.of(context).typography.bodyStrong),
                  subtitle: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        if (entry.summary.isNotEmpty)
                          Text(entry.summary,
                              maxLines: 2, overflow: TextOverflow.ellipsis),
                        const SizedBox(height: 4),
                        Text(
                          [
                            if (entry.version.isNotEmpty) 'v${entry.version}',
                            if (entry.license.isNotEmpty) entry.license,
                            if (entry.size >= 0) _formatSize(entry.size),
                            if (entry.authors.isNotEmpty)
                              entry.authors.join(', '),
                          ].join(' · '),
                          style: FluentTheme.of(context).typography.caption,
                        ),
                      ]),
                  trailing: FilledButton(
                    onPressed: _installing ? null : () => _install(entry),
                    child: const Text('安装'),
                  ),
                ),
              ),
            ),
        ],
      ],
    );
  }
}
