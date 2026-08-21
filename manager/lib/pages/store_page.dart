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
  final String description;
  final String license;
  final String minimumVersion;
  final List<String> authors;
  final List<String> tags;
  final List<String> categories;
  final int size;
  final String iconUrl;
  final String downloadUrl;
  final String sha256;

  _StorePageEntry.fromJson(Map<String, dynamic> json)
      : id = (json['id'] as String?) ?? '',
        name = (json['name'] as String?) ?? '',
        version = (json['version'] as String?) ?? '',
        summary = (json['summary'] as String?) ?? '',
        description = (json['description'] as String?) ?? '',
        license = (json['license'] as String?) ?? '',
        minimumVersion = (json['minimumNeurolingsCEVersion'] as String?) ?? '',
        authors = (json['authors'] as List?)?.map((e) => e.toString()).toList() ?? [],
        tags = (json['tags'] as List?)?.map((e) => e.toString()).toList() ?? [],
        categories = (json['categories'] as List?)?.map((e) => e.toString()).toList() ?? [],
        size = ((json['download'] as Map?)?['size'] as num?)?.toInt() ?? -1,
        iconUrl = ((json['icon'] as Map?)?['url'] as String?) ?? '',
        downloadUrl = ((json['download'] as Map?)?['url'] as String?) ?? '',
        sha256 = ((json['download'] as Map?)?['sha256'] as String?) ?? '';
}

class _StorePageState extends State<StorePage> {
  bool _loading = false;
  bool _installing = false;
  bool _configured = false;
  String _indexUrl = '';
  List<_StorePageEntry> _entries = [];
  String? _error;
  String _search = '';
  String _selectedTag = '';
  bool _fromCache = false;
  String? _warning;
  String? _installingId;

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
      final list = result['entries'] ?? result['mascots'];
      final fromCache = result['from_cache'] == true;
      final warning = result['warning'] is Map ? (result['warning']['error']?.toString()) : null;
      setState(() {
        _configured = true;
        _indexUrl = url;
        _fromCache = fromCache;
        _warning = warning;
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

  Future<void> _showDetail(_StorePageEntry entry) async {
    await showDialog(
      context: context,
      builder: (ctx) => ContentDialog(
        title: Text(entry.name),
        content: SizedBox(
          width: 480,
          child: SingleChildScrollView(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text('${entry.id}  ·  v${entry.version}', style: const TextStyle(fontSize: 12, color: Color(0xFF6B6B6B))),
              const SizedBox(height: 8),
              if (entry.summary.isNotEmpty) Text(entry.summary),
              if (entry.description.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 8), child: Text(entry.description, style: const TextStyle(fontSize: 13))),
              const SizedBox(height: 12),
              Wrap(spacing: 6, children: [
                if (entry.license.isNotEmpty) _chip('License: ${entry.license}'),
                if (entry.minimumVersion.isNotEmpty) _chip('最低版本: ${entry.minimumVersion}'),
                if (entry.size >= 0) _chip(_formatSize(entry.size)),
              ]),
              if (entry.authors.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 8), child: Text('作者: ${entry.authors.join(', ')}', style: const TextStyle(fontSize: 12))),
              if (entry.tags.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 4), child: Text('标签: ${entry.tags.join(', ')}', style: const TextStyle(fontSize: 12))),
              if (entry.categories.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 4), child: Text('分类: ${entry.categories.join(', ')}', style: const TextStyle(fontSize: 12))),
              if (entry.sha256.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 8), child: SelectableText('SHA256: ${entry.sha256}', style: const TextStyle(fontSize: 11, fontFamily: 'monospace'))),
            ]),
          ),
        ),
        actions: [
          Button(onPressed: () => Navigator.pop(ctx), child: const Text('关闭')),
          FilledButton(onPressed: () { Navigator.pop(ctx); _install(entry); }, child: const Text('安装')),
        ],
      ),
    );
  }

  Widget _chip(String label) => Container(padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4), decoration: BoxDecoration(color: const Color(0xFFF3F3F3), borderRadius: BorderRadius.circular(12)), child: Text(label, style: const TextStyle(fontSize: 11)));

  Future<void> _install(_StorePageEntry entry) async {
    setState(() { _installing = true; _installingId = entry.id; });
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
      if (mounted) setState(() { _installing = false; _installingId = null; });
    }
  }

  String _formatSize(int bytes) {
    if (bytes < 0) return '';
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
    return '${(bytes / 1024 / 1024).toStringAsFixed(1)} MB';
  }

  List<String> get _allTags {
    final set = <String>{};
    for (final e in _entries) {
      set.addAll(e.tags);
      set.addAll(e.categories);
    }
    final list = set.toList()..sort((a, b) => a.toLowerCase().compareTo(b.toLowerCase()));
    return list;
  }

  List<_StorePageEntry> get _filtered {
    final query = _search.trim().toLowerCase();
    final terms = query.split(RegExp(r'\s+')).where((s) => s.isNotEmpty).toList();
    return _entries.where((e) {
      if (_selectedTag.isNotEmpty) {
        final tagLower = _selectedTag.toLowerCase();
        final hasTag = e.tags.any((t) => t.toLowerCase() == tagLower) || e.categories.any((c) => c.toLowerCase() == tagLower);
        if (!hasTag) return false;
      }
      if (terms.isEmpty) return true;
      final haystack = '${e.name} ${e.summary} ${e.id} ${e.authors.join(' ')}'.toLowerCase();
      return terms.every((term) => haystack.contains(term));
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
          // 状态条 + 标签筛选
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Row(children: [
              Expanded(child: Text('${_filtered.length} 个桌宠${_selectedTag.isNotEmpty ? ' · 标签: $_selectedTag' : ''}${_fromCache ? ' · 来自缓存' : ''}', style: FluentTheme.of(context).typography.caption)),
              if (_warning != null) Icon(FluentIcons.warning, size: 14, color: Colors.orange),
            ]),
          ),
          if (_warning != null)
            Padding(padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4), child: InfoBar(title: const Text('缓存警告'), content: Text(_warning!), severity: InfoBarSeverity.warning)),
          if (_entries.isNotEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Row(children: [
                Expanded(child: TextBox(placeholder: '搜索名称、简介、ID或作者...', onChanged: (value) => setState(() => _search = value))),
                const SizedBox(width: 8),
                ComboBox<String>(
                  value: _selectedTag.isEmpty ? '' : _selectedTag,
                  placeholder: const Text('全部标签'),
                  items: [const ComboBoxItem(value: '', child: Text('全部标签')), ..._allTags.map((t) => ComboBoxItem(value: t, child: Text(t)))],
                  onChanged: (v) => setState(() => _selectedTag = v ?? ''),
                ),
              ]),
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
                  trailing: Row(mainAxisSize: MainAxisSize.min, children: [
                    Button(onPressed: () => _showDetail(entry), child: const Text('详情')),
                    const SizedBox(width: 6),
                    _installingId == entry.id
                        ? const SizedBox(width: 24, height: 24, child: ProgressRing(strokeWidth: 2))
                        : FilledButton(
                            onPressed: _installing ? null : () => _install(entry),
                            child: const Text('安装'),
                          ),
                  ]),
                  onPressed: () => _showDetail(entry),
                ),
              ),
            ),
          // GitHub 登录占位
          Card(
            margin: const EdgeInsets.all(16),
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Row(children: [const Icon(FluentIcons.accounts, size: 20), const SizedBox(width: 8), Text('GitHub 登录', style: FluentTheme.of(context).typography.bodyStrong)]),
                const SizedBox(height: 8),
                const Text('登录后可投稿、收藏，建议通过 NEUROLINGSCE_MASCOT_INDEX_URL 配置私有索引。', style: TextStyle(fontSize: 12)),
                const SizedBox(height: 8),
                Button(onPressed: () => displayInfoBar(context, builder: (c, close) => const InfoBar(title: Text('GitHub 登录 UI 正在接入'), content: Text('后端已实现 Device Flow，UI 接线进行中'), severity: InfoBarSeverity.info)), child: const Text('使用 GitHub 登录')),
                const SizedBox(height: 4),
                Button(onPressed: () => displayInfoBar(context, builder: (c, close) => const InfoBar(title: Text('投稿功能'), content: Text('后端 SubmissionClient 已就绪，UI 表单待补'), severity: InfoBarSeverity.info)), child: const Text('投稿新桌宠')),
              ]),
            ),
          ),
        ],
      ],
    );
  }
}
