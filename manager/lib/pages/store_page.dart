import 'dart:async';

import 'package:file_picker/file_picker.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter/services.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';
import 'package:url_launcher/url_launcher.dart';

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
      authors =
          (json['authors'] as List?)?.map((e) => e.toString()).toList() ?? [],
      tags = (json['tags'] as List?)?.map((e) => e.toString()).toList() ?? [],
      categories =
          (json['categories'] as List?)?.map((e) => e.toString()).toList() ??
          [],
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
  bool _loginConfigured = false;
  bool _signedIn = false;
  String _login = '';

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _load();
      _loadLogin();
    });
  }

  Future<void> _loadLogin() async {
    try {
      final state = context.read<AppState>();
      final status = await state.api.command({
        'command': 'store_github_status',
      });
      if (!mounted) return;
      setState(() {
        _loginConfigured = status['configured'] == true;
        _signedIn = status['signed_in'] == true;
        _login = (status['login'] as String?) ?? '';
      });
    } catch (_) {
      // 运行时离线时保持未登录展示。
    }
  }

  /// Device Flow 登录：显示用户码对话框并按提示间隔轮询授权结果。
  Future<void> _signIn() async {
    final l10n = AppLocalizations.of(context);
    final state = context.read<AppState>();
    try {
      final start = await state.api.command({'command': 'store_github_start'});
      if (!mounted) return;
      final userCode = start['user_code'] as String? ?? '';
      final uri = start['verification_uri'] as String? ?? '';
      var interval = (start['interval'] as num?)?.toInt() ?? 5;
      var closed = false;
      final completer = Completer<void>();
      unawaited(
        showDialog<void>(
          context: context,
          barrierDismissible: false,
          builder: (dialogContext) {
            Future<void> poll() async {
              while (!closed && !completer.isCompleted) {
                await Future.delayed(Duration(seconds: interval));
                if (closed) return;
                try {
                  final step = await state.api.command({
                    'command': 'store_github_step',
                  });
                  if (step['state'] == 'authorized') {
                    if (!completer.isCompleted) completer.complete();
                    return;
                  }
                  if (step['state'] == 'pending') {
                    interval =
                        (step['next_interval'] as num?)?.toInt() ?? interval;
                  } else {
                    if (!completer.isCompleted) completer.complete();
                    return;
                  }
                } catch (_) {
                  if (!completer.isCompleted) completer.complete();
                  return;
                }
              }
            }

            unawaited(poll());
            completer.future.whenComplete(() {
              closed = true;
              if (dialogContext.mounted && Navigator.canPop(dialogContext)) {
                Navigator.pop(dialogContext);
              }
            });
            return ContentDialog(
              title: Text(l10n.storeSignIn),
              content: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(l10n.storeSignInHint),
                  const SizedBox(height: 12),
                  Center(
                    child: SelectableText(
                      userCode,
                      style: FluentTheme.of(dialogContext).typography.display
                          ?.copyWith(fontWeight: FontWeight.w600),
                    ),
                  ),
                  const SizedBox(height: 12),
                  Center(
                    child: HyperlinkButton(
                      onPressed: () => launchUrl(Uri.parse(uri)),
                      child: Text(uri),
                    ),
                  ),
                  const SizedBox(height: 8),
                  Center(
                    child: Button(
                      onPressed: () {
                        Clipboard.setData(ClipboardData(text: userCode));
                      },
                      child: Text(l10n.storeCopyCode),
                    ),
                  ),
                  const SizedBox(height: 12),
                  const Center(
                    child: SizedBox(
                      width: 20,
                      height: 20,
                      child: ProgressRing(strokeWidth: 2),
                    ),
                  ),
                ],
              ),
              actions: [
                Button(
                  onPressed: () {
                    closed = true;
                    if (!completer.isCompleted) completer.complete();
                    Navigator.pop(dialogContext);
                  },
                  child: Text(l10n.cancel),
                ),
              ],
            );
          },
        ),
      );
      await completer.future.catchError((_) {});
      await _loadLogin();
      if (!mounted) return;
      displayInfoBar(
        context,
        builder: (ctx, close) {
          return InfoBar(
            title: Text(
              _signedIn ? l10n.storeSignInDone(_login) : l10n.storeSignInFailed,
            ),
            severity: _signedIn
                ? InfoBarSeverity.success
                : InfoBarSeverity.warning,
          );
        },
      );
    } catch (e) {
      if (!mounted) return;
      displayInfoBar(
        context,
        builder: (ctx, close) {
          return InfoBar(
            title: Text(l10n.error),
            content: Text(e.toString()),
            severity: InfoBarSeverity.error,
          );
        },
      );
    }
  }

  Future<void> _signOut() async {
    final state = context.read<AppState>();
    try {
      await state.api.command({'command': 'store_github_signout'});
    } catch (_) {}
    await _loadLogin();
  }

  /// 投稿对话框：包路径 + 元数据表单 + 提交（对齐原版 MascotSubmissionDialog）。
  Future<void> _submitMascot() async {
    final l10n = AppLocalizations.of(context);
    final state = context.read<AppState>();
    if (!_signedIn) {
      await _signIn();
      if (!mounted) return;
      if (!_signedIn) return;
    }
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => _SubmissionDialog(
        l10n: l10n,
        onSubmit: (fields) async {
          return state.api.command({
            'command': 'store_submit_mascot',
            ...fields,
          });
        },
      ),
    );
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

      final result = await state.api.command({
        'command': 'store_index',
        'refresh': refresh,
      });
      if (!mounted) return;
      final list = result['entries'] ?? result['mascots'];
      final fromCache = result['from_cache'] == true;
      final warning = result['warning'] is Map
          ? (result['warning']['error']?.toString())
          : null;
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
    final l10n = AppLocalizations.of(context);
    await showDialog(
      context: context,
      builder: (ctx) => ContentDialog(
        title: Text(entry.name),
        content: SizedBox(
          width: 480,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '${entry.id}  ·  v${entry.version}',
                  style: const TextStyle(
                    fontSize: 12,
                    color: Color(0xFF6B6B6B),
                  ),
                ),
                const SizedBox(height: 8),
                if (entry.summary.isNotEmpty) Text(entry.summary),
                if (entry.description.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Text(
                      entry.description,
                      style: const TextStyle(fontSize: 13),
                    ),
                  ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 6,
                  children: [
                    if (entry.license.isNotEmpty)
                      _chip('License: ${entry.license}'),
                    if (entry.minimumVersion.isNotEmpty)
                      _chip(l10n.storeMinVersion(entry.minimumVersion)),
                    if (entry.size >= 0) _chip(_formatSize(entry.size)),
                  ],
                ),
                if (entry.authors.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Text(
                      l10n.storeAuthors(entry.authors.join(', ')),
                      style: const TextStyle(fontSize: 12),
                    ),
                  ),
              ],
            ),
          ),
        ),
        actions: [
          Button(onPressed: () => Navigator.pop(ctx), child: Text(l10n.close)),
          FilledButton(
            onPressed: () {
              Navigator.pop(ctx);
              _install(entry);
            },
            child: Text(l10n.storeInstall),
          ),
        ],
      ),
    );
  }

  Widget _chip(String label) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
    decoration: BoxDecoration(
      color: const Color(0xFFF3F3F3),
      borderRadius: BorderRadius.circular(12),
    ),
    child: Text(label, style: const TextStyle(fontSize: 11)),
  );

  Future<void> _install(_StorePageEntry entry) async {
    setState(() {
      _installing = true;
      _installingId = entry.id;
    });
    try {
      final state = context.read<AppState>();
      final result = await state.api.command({
        'command': 'store_install',
        'id': entry.id,
      });
      if (!mounted) return;
      await state.refresh();
      if (!mounted) return;
      final entryInfo = result['store_entry'];
      final name = entryInfo is Map
          ? (entryInfo['name'] ?? entry.name)
          : entry.name;
      await displayInfoBar(
        context,
        builder: (context, close) {
          return InfoBar(
            title: Text(AppLocalizations.of(context).storeInstallOk('$name')),
            content: Text(AppLocalizations.of(context).storeInstallOkHint),
            severity: InfoBarSeverity.success,
            action: IconButton(
              icon: const Icon(FluentIcons.clear),
              onPressed: close,
            ),
          );
        },
      );
    } catch (e) {
      if (!mounted) return;
      await displayInfoBar(
        context,
        builder: (context, close) {
          return InfoBar(
            title: Text(
              AppLocalizations.of(context).storeInstallFailed(entry.name),
            ),
            content: Text(e.toString()),
            severity: InfoBarSeverity.error,
            action: IconButton(
              icon: const Icon(FluentIcons.clear),
              onPressed: close,
            ),
          );
        },
      );
    } finally {
      if (mounted) {
        setState(() {
          _installing = false;
          _installingId = null;
        });
      }
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
    final list = set.toList()
      ..sort((a, b) => a.toLowerCase().compareTo(b.toLowerCase()));
    return list;
  }

  List<_StorePageEntry> get _filtered {
    final query = _search.trim().toLowerCase();
    final terms = query
        .split(RegExp(r'\s+'))
        .where((s) => s.isNotEmpty)
        .toList();
    return _entries.where((e) {
      if (_selectedTag.isNotEmpty) {
        final tagLower = _selectedTag.toLowerCase();
        final hasTag =
            e.tags.any((t) => t.toLowerCase() == tagLower) ||
            e.categories.any((c) => c.toLowerCase() == tagLower);
        if (!hasTag) return false;
      }
      if (terms.isEmpty) return true;
      final haystack = '${e.name} ${e.summary} ${e.id} ${e.authors.join(' ')}'
          .toLowerCase();
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
            child: Row(
              children: [
                const Icon(FluentIcons.shop, size: 24),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        l10n.storeIndex,
                        style: FluentTheme.of(context).typography.bodyStrong,
                      ),
                      const SizedBox(height: 4),
                      SelectableText(
                        _indexUrl.isEmpty ? l10n.storeNotConfigured : _indexUrl,
                        style: FluentTheme.of(context).typography.caption,
                      ),
                    ],
                  ),
                ),
                IconButton(
                  icon: const Icon(FluentIcons.refresh),
                  onPressed: _loading ? null : () => _load(refresh: true),
                ),
              ],
            ),
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
              title: Text(l10n.storeRuntimeOffline),
              content: Text(l10n.storeRuntimeOfflineHint),
              severity: InfoBarSeverity.warning,
            ),
          )
        else if (!_configured)
          Card(
            margin: const EdgeInsets.all(16),
            child: Padding(
              padding: const EdgeInsets.all(32),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(FluentIcons.shop, size: 48),
                  const SizedBox(height: 16),
                  Text(
                    l10n.storeUnconfigured,
                    style: FluentTheme.of(context).typography.bodyLarge,
                  ),
                  const SizedBox(height: 8),
                  Text(l10n.storeUnconfiguredHint, textAlign: TextAlign.center),
                ],
              ),
            ),
          )
        else if (_error != null)
          Card(
            margin: const EdgeInsets.all(16),
            child: InfoBar(
              title: Text(l10n.storeLoadFailed),
              content: Text(_error!),
              severity: InfoBarSeverity.error,
            ),
          )
        else ...[
          // 状态条 + 标签筛选
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    [
                      l10n.storeCount(_filtered.length),
                      if (_selectedTag.isNotEmpty) l10n.storeTag(_selectedTag),
                      if (_fromCache) l10n.storeFromCache,
                    ].join(' · '),
                    style: FluentTheme.of(context).typography.caption,
                  ),
                ),
                if (_warning != null)
                  Icon(FluentIcons.warning, size: 14, color: Colors.orange),
              ],
            ),
          ),
          if (_warning != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
              child: InfoBar(
                title: Text(l10n.storeCacheWarning),
                content: Text(_warning!),
                severity: InfoBarSeverity.warning,
              ),
            ),
          if (_entries.isNotEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Row(
                children: [
                  Expanded(
                    child: TextBox(
                      placeholder: l10n.storeSearchHint,
                      onChanged: (value) => setState(() => _search = value),
                    ),
                  ),
                  const SizedBox(width: 8),
                  ComboBox<String>(
                    value: _selectedTag.isEmpty ? '' : _selectedTag,
                    placeholder: Text(l10n.storeAllTags),
                    items: [
                      ComboBoxItem(value: '', child: Text(l10n.storeAllTags)),
                      ..._allTags.map(
                        (t) => ComboBoxItem(value: t, child: Text(t)),
                      ),
                    ],
                    onChanged: (v) => setState(() => _selectedTag = v ?? ''),
                  ),
                ],
              ),
            ),
          if (_filtered.isEmpty)
            Card(
              margin: const EdgeInsets.all(16),
              child: Padding(
                padding: const EdgeInsets.all(32),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(FluentIcons.search, size: 48),
                    const SizedBox(height: 16),
                    Text(
                      _entries.isEmpty ? l10n.storeEmpty : l10n.storeNoMatch,
                      style: FluentTheme.of(context).typography.bodyLarge,
                    ),
                  ],
                ),
              ),
            )
          else
            ..._filtered.map(
              (entry) => Card(
                margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                child: ListTile(
                  title: Text(
                    entry.name,
                    style: FluentTheme.of(context).typography.bodyStrong,
                  ),
                  subtitle: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      if (entry.summary.isNotEmpty)
                        Text(
                          entry.summary,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                        ),
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
                    ],
                  ),
                  trailing: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Button(
                        onPressed: () => _showDetail(entry),
                        child: Text(l10n.storeDetails),
                      ),
                      const SizedBox(width: 6),
                      _installingId == entry.id
                          ? const SizedBox(
                              width: 24,
                              height: 24,
                              child: ProgressRing(strokeWidth: 2),
                            )
                          : FilledButton(
                              onPressed: _installing
                                  ? null
                                  : () => _install(entry),
                              child: Text(l10n.storeInstall),
                            ),
                    ],
                  ),
                  onPressed: () => _showDetail(entry),
                ),
              ),
            ),
          // 社区投稿区（对齐原版：登录状态 + 投稿入口）
          Card(
            margin: const EdgeInsets.all(16),
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      const Icon(FluentIcons.accounts, size: 20),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          _signedIn
                              ? l10n.storeSignedInAs(_login)
                              : l10n.storeCommunity,
                          style: FluentTheme.of(context).typography.bodyStrong,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  Text(
                    _signedIn
                        ? l10n.storeCommunityHintSignedIn
                        : l10n.storeCommunityHint,
                    style: const TextStyle(fontSize: 12),
                  ),
                  const SizedBox(height: 8),
                  if (!_signedIn)
                    Button(
                      onPressed: _loginConfigured ? _signIn : null,
                      child: Text(
                        _loginConfigured
                            ? l10n.storeSignIn
                            : l10n.storeSignInUnavailable,
                      ),
                    )
                  else ...[
                    Button(
                      onPressed: _submitMascot,
                      child: Text(l10n.storeSubmit),
                    ),
                    const SizedBox(height: 4),
                    Button(onPressed: _signOut, child: Text(l10n.storeSignOut)),
                  ],
                ],
              ),
            ),
          ),
        ],
      ],
    );
  }
}

/// 投稿表单对话框：选择 .mascot 包并填写元数据后提交。
class _SubmissionDialog extends StatefulWidget {
  const _SubmissionDialog({required this.l10n, required this.onSubmit});

  final AppLocalizations l10n;
  final Future<Map<String, dynamic>> Function(Map<String, dynamic>) onSubmit;

  @override
  State<_SubmissionDialog> createState() => _SubmissionDialogState();
}

class _SubmissionDialogState extends State<_SubmissionDialog> {
  String? _packagePath;
  final _id = TextEditingController();
  final _name = TextEditingController();
  final _version = TextEditingController();
  final _summary = TextEditingController();
  final _description = TextEditingController();
  final _license = TextEditingController(text: 'CC-BY-NC-SA-4.0');
  final _maintainers = TextEditingController();
  bool _confirmed = false;
  bool _submitting = false;
  String? _result;

  @override
  void dispose() {
    for (final controller in [
      _id,
      _name,
      _version,
      _summary,
      _description,
      _license,
      _maintainers,
    ]) {
      controller.dispose();
    }
    super.dispose();
  }

  Future<void> _pickPackage() async {
    final picked = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['mascot'],
      dialogTitle: widget.l10n.storeSubmitPickPackage,
    );
    if (picked == null) return;
    final path = picked.files.singleOrNull?.path;
    if (path == null) return;
    setState(() => _packagePath = path);
    // 以文件名预填 id。
    if (_id.text.trim().isEmpty) {
      final base = path.split(RegExp(r'[\/]')).last;
      _id.text = base.replaceFirst(
        RegExp(r'\.mascot$', caseSensitive: false),
        '',
      );
    }
  }

  Future<void> _submit() async {
    if (_packagePath == null || !_confirmed) return;
    setState(() => _submitting = true);
    try {
      final result = await widget.onSubmit({
        'path': _packagePath,
        'id': _id.text.trim(),
        'name': _name.text.trim(),
        'version': _version.text.trim(),
        'summary': _summary.text.trim(),
        'description': _description.text.trim(),
        'license': _license.text.trim(),
        'maintainers': _maintainers.text.trim(),
      });
      if (!mounted) return;
      final ok = result['ok'] == true;
      final prUrl = result['pr_url'] as String? ?? '';
      setState(
        () => _result = ok
            ? widget.l10n.storeSubmitDone(prUrl)
            : widget.l10n.storeSubmitFailed(
                result['error_code'] as String? ?? '',
                result['error'] as String? ?? '',
              ),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() => _result = e.toString());
    } finally {
      if (mounted) setState(() => _submitting = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = widget.l10n;
    return ContentDialog(
      title: Text(l10n.storeSubmit),
      content: SizedBox(
        width: 460,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: TextBox(
                      readOnly: true,
                      placeholder: l10n.storeSubmitPickPackage,
                      controller: TextEditingController(
                        text: _packagePath ?? '',
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Button(
                    onPressed: _pickPackage,
                    child: Text(l10n.storeSubmitPick),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              _field('ID', _id),
              _field(l10n.storeSubmitName, _name),
              _field(l10n.version, _version),
              _field(l10n.storeSubmitSummary, _summary),
              _field(l10n.description, _description),
              _field(l10n.license, _license),
              _field(l10n.storeSubmitMaintainers, _maintainers),
              const SizedBox(height: 8),
              Checkbox(
                checked: _confirmed,
                onChanged: (v) => setState(() => _confirmed = v ?? false),
                content: Text(l10n.storeSubmitConfirm),
              ),
              if (_result != null) ...[
                const SizedBox(height: 8),
                Text(_result!),
              ],
            ],
          ),
        ),
      ),
      actions: [
        Button(
          onPressed: () => Navigator.pop(context),
          child: Text(l10n.close),
        ),
        FilledButton(
          onPressed: _submitting || _packagePath == null || !_confirmed
              ? null
              : _submit,
          child: _submitting
              ? const SizedBox(
                  width: 14,
                  height: 14,
                  child: ProgressRing(strokeWidth: 2),
                )
              : Text(l10n.storeSubmit),
        ),
      ],
    );
  }

  Widget _field(String label, TextEditingController controller) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          SizedBox(width: 90, child: Text(label)),
          Expanded(child: TextBox(controller: controller)),
        ],
      ),
    );
  }
}
