import 'package:file_picker/file_picker.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

/// Create page: import a Shimeji-ee zip / .mascot via the standalone CLI.
class CreatePage extends StatefulWidget {
  const CreatePage({super.key});

  @override
  State<CreatePage> createState() => _CreatePageState();
}

class _CreatePageState extends State<CreatePage> {
  String? _result;
  bool _importing = false;

  Future<void> _pick() async {
    final l10n = AppLocalizations.of(context);
    final picked = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['zip', 'mascot', 'rar', '7z'],
    );
    if (picked == null || picked.files.isEmpty) return;
    final path = picked.files.first.path;
    if (path == null || !mounted) return;

    setState(() {
      _importing = true;
      _result = null;
    });
    final state = context.read<AppState>();
    final output = await state.importArchive(path);
    if (!mounted) return;
    setState(() {
      _importing = false;
      _result = output.isEmpty ? l10n.ok : output;
    });
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.createTitle)),
      children: [
        Text(l10n.createHint),
        const SizedBox(height: 16),
        FilledButton(
          onPressed: _importing ? null : _pick,
          child: Text(_importing ? l10n.importing : l10n.pickArchive),
        ),
        if (_result != null) ...[
          const SizedBox(height: 16),
          Card(
            child: SelectableText(
              l10n.importDone(_result!),
              style: FluentTheme.of(context).typography.caption,
            ),
          ),
        ],
      ],
    );
  }
}
