import 'package:file_picker/file_picker.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../api/runtime_api.dart';
import '../state/app_state.dart';

/// Create page: 三段式向导（对齐原版 ManagerCreatePage.cc）
/// 1. 选择归档（zip/mascot）
/// 2. 检查内容（validate）+ 预览候选
/// 3. 生成/导入并回显结果
class CreatePage extends StatefulWidget {
  const CreatePage({super.key});

  @override
  State<CreatePage> createState() => _CreatePageState();
}

class _CreatePageState extends State<CreatePage> {
  String? _pickedPath;
  String? _validateOutput;
  String? _importResult;
  bool _busy = false;
  bool _validating = false;

  Future<void> _pick() async {
    final picked = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['zip', 'mascot', 'rar', '7z'],
    );
    if (picked == null || picked.files.isEmpty) return;
    final path = picked.files.first.path;
    if (path == null || !mounted) return;
    setState(() {
      _pickedPath = path;
      _validateOutput = null;
      _importResult = null;
    });
    await _validate();
  }

  Future<void> _validate() async {
    if (_pickedPath == null) return;
    setState(() => _validating = true);
    try {
      final (code, out, err) = await runCli(['--mascot', 'validate', _pickedPath!]);
      if (!mounted) return;
      setState(() {
        _validating = false;
        final combined = (out + '\n' + err).trim();
        _validateOutput = combined.isEmpty
            ? (code == 0 ? '校验通过：包格式合法' : '校验失败 (exit $code)')
            : combined;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _validating = false;
        _validateOutput = '校验异常: $e';
      });
    }
  }

  Future<void> _import() async {
    if (_pickedPath == null) return;
    setState(() => _busy = true);
    try {
      final state = context.read<AppState>();
      final output = await state.importArchive(_pickedPath!);
      if (!mounted) return;
      final l10n = AppLocalizations.of(context);
      setState(() {
        _busy = false;
        _importResult = output.isEmpty ? l10n.ok : output;
      });
      await displayInfoBar(context, builder: (c, close) => InfoBar(
            title: const Text('已导入'),
            content: Text(_importResult!),
            severity: InfoBarSeverity.success,
          ));
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _importResult = e.toString();
      });
      await displayInfoBar(context, builder: (c, close) => InfoBar(
            title: const Text('导入失败'),
            content: Text(e.toString()),
            severity: InfoBarSeverity.error,
          ));
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.createTitle)),
      children: [
        // 步骤说明
        InfoBar(
          title: const Text('三步完成桌宠创建'),
          content: const Text('1 选择归档 → 2 检查内容 → 3 生成 .mascot 并导入模板库'),
          severity: InfoBarSeverity.info,
        ),
        const SizedBox(height: 16),

        // Step 1
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                Container(
                  width: 28,
                  height: 28,
                  decoration: BoxDecoration(color: FluentTheme.of(context).accentColor, borderRadius: BorderRadius.circular(14)),
                  child: const Center(child: Text('1', style: TextStyle(color: Colors.white))),
                ),
                const SizedBox(width: 12),
                Text('选择归档', style: FluentTheme.of(context).typography.bodyStrong),
              ]),
              const SizedBox(height: 12),
              Text(l10n.createHint),
              const SizedBox(height: 12),
              Row(children: [
                FilledButton(
                  onPressed: _busy ? null : _pick,
                  child: const Text('选择 Zip / Mascot...'),
                ),
                const SizedBox(width: 12),
                if (_pickedPath != null)
                  Expanded(child: SelectableText(_pickedPath!, style: FluentTheme.of(context).typography.caption)),
              ]),
            ]),
          ),
        ),
        const SizedBox(height: 12),

        // Step 2
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                Container(
                  width: 28,
                  height: 28,
                  decoration: BoxDecoration(color: _pickedPath == null ? Colors.grey : FluentTheme.of(context).accentColor, borderRadius: BorderRadius.circular(14)),
                  child: Center(child: Text('2', style: TextStyle(color: Colors.white))),
                ),
                const SizedBox(width: 12),
                Text('检查内容', style: FluentTheme.of(context).typography.bodyStrong),
                const Spacer(),
                Button(
                  onPressed: _pickedPath == null || _validating ? null : _validate,
                  child: _validating ? const SizedBox(width: 14, height: 14, child: ProgressRing(strokeWidth: 2)) : const Text('重新校验'),
                ),
              ]),
              const SizedBox(height: 12),
              if (_pickedPath == null)
                const Text('请先选择归档文件', style: TextStyle(color: Color(0xFF6B6B6B)))
              else if (_validating)
                const Row(children: [ProgressRing(), SizedBox(width: 8), Text('正在校验...')])
              else if (_validateOutput != null)
                Container(
                  width: double.infinity,
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(color: const Color(0xFFF3F3F3), borderRadius: BorderRadius.circular(6)),
                  child: SelectableText(_validateOutput!, style: const TextStyle(fontFamily: 'monospace', fontSize: 12)),
                )
              else
                const Text('—'),
              const SizedBox(height: 8),
              const Text('提示：支持 .zip 与 .mascot；rar/7z 将尝试兼容导入，但建议先转为 zip。', style: TextStyle(fontSize: 12, color: Color(0xFF6B6B6B))),
            ]),
          ),
        ),
        const SizedBox(height: 12),

        // Step 3
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Row(children: [
                Container(
                  width: 28,
                  height: 28,
                  decoration: BoxDecoration(color: _pickedPath == null ? Colors.grey : FluentTheme.of(context).accentColor, borderRadius: BorderRadius.circular(14)),
                  child: const Center(child: Text('3', style: TextStyle(color: Colors.white))),
                ),
                const SizedBox(width: 12),
                Text('生成与导入', style: FluentTheme.of(context).typography.bodyStrong),
              ]),
              const SizedBox(height: 12),
              const Text('导入后模板将出现在主页“已安装”列表，可直接召唤。', style: TextStyle(fontSize: 12, color: Color(0xFF6B6B6B))),
              const SizedBox(height: 12),
              FilledButton(
                onPressed: _pickedPath == null || _busy ? null : _import,
                child: _busy ? const Row(mainAxisSize: MainAxisSize.min, children: [SizedBox(width: 14, height: 14, child: ProgressRing(strokeWidth: 2)), SizedBox(width: 8), Text('导入中...')]) : const Text('生成 .mascot 并导入'),
              ),
              if (_importResult != null) ...[
                const SizedBox(height: 12),
                Container(
                  width: double.infinity,
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(color: const Color(0xFFF3F3F3), borderRadius: BorderRadius.circular(6)),
                  child: SelectableText(_importResult!, style: FluentTheme.of(context).typography.caption),
                ),
              ],
            ]),
          ),
        ),
      ],
    );
  }
}
