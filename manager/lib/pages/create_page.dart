import 'dart:async';
import 'dart:convert';

import 'package:file_picker/file_picker.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

/// 创建页：旧版 Shimeji zip → .mascot 转换器（对齐原版 ManagerCreatePage）。
/// 三步：选择 zip 并检查内容 → 勾选候选并编辑 info.json → 选择输出目录生成。
/// 产物只写入所选目录，不导入存储。
class CreatePage extends StatefulWidget {
  const CreatePage({super.key});

  @override
  State<CreatePage> createState() => _CreatePageState();
}

/// 单个候选的编辑状态。
class _Candidate {
  _Candidate({required this.raw});

  final Map<String, dynamic> raw;
  bool selected = false;
  late final TextEditingController infoController = TextEditingController(
    text: raw['info_json'] as String? ?? '',
  );
  Timer? _debounce;
  String validation = '';
  bool get convertible => raw['convertible'] == true;
  String get name => raw['name'] as String? ?? '';

  void dispose() {
    _debounce?.cancel();
    infoController.dispose();
  }
}

class _CreatePageState extends State<CreatePage> {
  String? _archivePath;
  bool _analyzing = false;
  bool _converting = false;
  String? _error;
  List<_Candidate> _candidates = [];
  String? _outputDir;
  String _resultText = '';

  void _validateDebounced(_Candidate candidate) {
    candidate._debounce?.cancel();
    candidate._debounce = Timer(const Duration(milliseconds: 300), () {
      final l10n = AppLocalizations.of(context);
      if (!mounted) return;
      setState(() {
        final text = candidate.infoController.text.trim();
        if (text.isEmpty) {
          candidate.validation = l10n.createInvalidJson('');
          return;
        }
        try {
          final decoded = jsonDecode(text);
          if (decoded is! Map<String, dynamic>) {
            candidate.validation = l10n.createInvalidJson('not an object');
          } else {
            final name = decoded['name'];
            if (name is! String || name.trim().isEmpty) {
              candidate.validation = l10n.createInvalidJson('name is required');
            } else {
              candidate.validation = l10n.createValidJson;
            }
          }
        } catch (e) {
          candidate.validation = l10n.createInvalidJson(e.toString());
        }
      });
    });
  }

  Future<void> _chooseArchive() async {
    final picked = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['zip'],
      dialogTitle: AppLocalizations.of(context).createChooseZip,
    );
    if (picked == null) return;
    final path = picked.files.singleOrNull?.path;
    if (path == null) return;
    setState(() {
      _archivePath = path;
      _candidates = [];
      _resultText = '';
      _error = null;
    });
  }

  Future<void> _checkContent() async {
    if (_archivePath == null) return;
    final l10n = AppLocalizations.of(context);
    setState(() {
      _analyzing = true;
      _error = null;
    });
    try {
      final state = context.read<AppState>();
      final result = await state.api.command({
        'command': 'analyze_archive',
        'path': _archivePath,
      });
      if (!mounted) return;
      final list = result['candidates'];
      final candidates = <_Candidate>[];
      if (list is List) {
        for (final entry in list) {
          if (entry is Map) {
            final candidate = _Candidate(raw: entry.cast<String, dynamic>());
            // info.json 有效的候选默认勾选（对齐原版）。
            candidate.selected =
                candidate.convertible && entry['info_json_valid'] == true;
            candidates.add(candidate);
          }
        }
      }
      setState(() {
        _candidates = candidates;
        if (result['ok'] != true) {
          _error = (result['error'] as String?)?.isNotEmpty == true
              ? result['error'] as String
              : l10n.createNoCandidates;
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _analyzing = false);
    }
  }

  Future<void> _chooseFolder() async {
    final picked = await FilePicker.platform.getDirectoryPath(
      dialogTitle: AppLocalizations.of(context).createChooseFolder,
    );
    if (picked == null) return;
    setState(() => _outputDir = picked);
  }

  Future<void> _generate() async {
    final l10n = AppLocalizations.of(context);
    final selected = _candidates
        .where((c) => c.selected && c.convertible)
        .toList();
    if (selected.isEmpty || _outputDir == null) return;
    setState(() {
      _converting = true;
      _resultText = '';
    });
    try {
      final state = context.read<AppState>();
      final selections = selected
          .map((c) => {'name': c.name, 'info_json': c.infoController.text})
          .toList();
      final result = await state.api.command({
        'command': 'convert_archive',
        'path': _archivePath,
        'out_dir': _outputDir,
        'selections': selections,
      });
      if (!mounted) return;
      final lines = <String>[];
      final results = result['results'];
      if (results is List) {
        for (final entry in results) {
          if (entry is! Map) continue;
          final name = entry['name'] ?? '';
          if (entry['ok'] == true) {
            lines.add(l10n.createCreated(name));
          } else {
            lines.add(l10n.createFailed(name, entry['error'] ?? ''));
          }
        }
      }
      lines.add(
        l10n.createConvertedCount((result['created'] as num?)?.toInt() ?? 0),
      );
      setState(() => _resultText = lines.join('\n'));
    } catch (e) {
      if (!mounted) return;
      setState(() => _resultText = e.toString());
    } finally {
      if (mounted) setState(() => _converting = false);
    }
  }

  @override
  void dispose() {
    for (final candidate in _candidates) {
      candidate.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.createTitle)),
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Text(
            l10n.createHint,
            style: FluentTheme.of(context).typography.caption,
          ),
        ),
        const SizedBox(height: 12),
        _stepCard(
          context,
          index: 1,
          title: l10n.createStep1,
          child: Row(
            children: [
              Expanded(
                child: TextBox(
                  readOnly: true,
                  placeholder: l10n.createChooseZip,
                  controller: TextEditingController(text: _archivePath ?? ''),
                ),
              ),
              const SizedBox(width: 8),
              Button(
                onPressed: _analyzing ? null : _chooseArchive,
                child: Text(l10n.createChooseZip),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: _analyzing || _archivePath == null
                    ? null
                    : _checkContent,
                child: _analyzing
                    ? const SizedBox(
                        width: 14,
                        height: 14,
                        child: ProgressRing(strokeWidth: 2),
                      )
                    : Text(l10n.createCheckContent),
              ),
            ],
          ),
        ),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.all(16),
            child: InfoBar(
              title: Text(l10n.error),
              content: Text(_error!),
              severity: InfoBarSeverity.warning,
            ),
          ),
        if (_candidates.isNotEmpty)
          _stepCard(
            context,
            index: 2,
            title: l10n.createStep2,
            child: Column(
              children: [
                for (final candidate in _candidates)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 6),
                    child: _candidateEditor(context, candidate),
                  ),
              ],
            ),
          ),
        _stepCard(
          context,
          index: 3,
          title: l10n.createStep3,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: TextBox(
                      readOnly: true,
                      placeholder: l10n.createChooseFolder,
                      controller: TextEditingController(text: _outputDir ?? ''),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Button(
                    onPressed: _chooseFolder,
                    child: Text(l10n.createChooseFolder),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              FilledButton(
                onPressed:
                    _converting ||
                        _outputDir == null ||
                        !_candidates.any((c) => c.selected && c.convertible)
                    ? null
                    : _generate,
                child: _converting
                    ? const SizedBox(
                        width: 14,
                        height: 14,
                        child: ProgressRing(strokeWidth: 2),
                      )
                    : Text(l10n.createGenerate),
              ),
              if (_resultText.isNotEmpty) ...[
                const SizedBox(height: 12),
                TextBox(
                  readOnly: true,
                  maxLines: null,
                  controller: TextEditingController(text: _resultText),
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }

  Widget _stepCard(
    BuildContext context, {
    required int index,
    required String title,
    required Widget child,
  }) {
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Text(
                  '$index. ',
                  style: FluentTheme.of(context).typography.bodyStrong,
                ),
                Text(
                  title,
                  style: FluentTheme.of(context).typography.bodyStrong,
                ),
              ],
            ),
            const SizedBox(height: 10),
            child,
          ],
        ),
      ),
    );
  }

  Widget _candidateEditor(BuildContext context, _Candidate candidate) {
    final l10n = AppLocalizations.of(context);
    final warnings = (candidate.raw['warnings'] as List?)?.cast<String>() ?? [];
    final tooltip = StringBuffer()
      ..write(candidate.name)
      ..write('\nVersion: ')
      ..write(candidate.raw['version'] ?? '')
      ..write('\nAuthor: ')
      ..write(candidate.raw['author'] ?? '');
    return Container(
      decoration: BoxDecoration(
        border: Border.all(color: Colors.grey.withValues(alpha: 0.3)),
        borderRadius: BorderRadius.circular(7),
      ),
      padding: const EdgeInsets.all(10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Checkbox(
                checked: candidate.selected,
                onChanged: candidate.convertible
                    ? (v) => setState(() => candidate.selected = v ?? false)
                    : null,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Tooltip(
                  message: tooltip.toString(),
                  child: Text(
                    candidate.name +
                        (candidate.convertible
                            ? ''
                            : '  (${l10n.createNotConvertible})'),
                    style: FluentTheme.of(context).typography.bodyStrong,
                  ),
                ),
              ),
            ],
          ),
          if (warnings.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(
                warnings.join('; '),
                style: FluentTheme.of(context).typography.caption,
              ),
            ),
          if (candidate.convertible) ...[
            const SizedBox(height: 8),
            SizedBox(
              height: 180,
              child: TextBox(
                maxLines: null,
                controller: candidate.infoController,
                onChanged: (_) => _validateDebounced(candidate),
                style: FluentTheme.of(context).typography.body?.copyWith(
                  fontFamily: 'Consolas, monospace',
                  fontSize: 12,
                ),
              ),
            ),
            const SizedBox(height: 4),
            Text(
              candidate.validation,
              style: FluentTheme.of(context).typography.caption,
            ),
          ],
        ],
      ),
    );
  }
}
