import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter/services.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';
import 'package:url_launcher/url_launcher.dart';

import '../state/app_state.dart';

const String qqGroup = '125081756';
const String githubUrl = 'https://github.com/qingchenyouforcc/NeurolingsCE';
const String upstreamUrl = 'https://github.com/pixelomer/Shijima-Qt';
const String issuesUrl = 'https://github.com/qingchenyouforcc/NeurolingsCE/issues';
const String releasesUrl = '$githubUrl/releases/latest';

/// 关于页：版本卡 / 更新卡 / 项目卡（对齐原版 ManagerAboutSection）。
class AboutPage extends StatefulWidget {
  const AboutPage({super.key});

  @override
  State<AboutPage> createState() => _AboutPageState();
}

class _AboutPageState extends State<AboutPage> {
  String _version = '';
  String? _latest;
  bool _checked = false;
  bool _notify = false;
  bool _downloading = false;
  String _downloadedVersion = '';
  String _updateError = '';

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadVersion());
    _pollUpdateStatus();
  }

  Future<void> _pollUpdateStatus() async {
    try {
      final status = await context
          .read<AppState>()
          .api
          .command({'command': 'update_status'});
      if (!mounted) return;
      setState(() {
        _checked = status['checked'] == true;
        _notify = status['notify'] == true;
        final latest = status['latest_version'] as String? ?? '';
        _latest = latest.isEmpty ? null : latest;
        _downloading = status['downloading'] == true;
        _downloadedVersion = status['downloaded_version'] as String? ?? '';
        _updateError = status['error'] as String? ?? '';
      });
    } catch (_) {}
  }

  Future<void> _checkForUpdates() async {
    setState(() => _updateError = '');
    try {
      await context.read<AppState>().api.command({'command': 'update_check'});
    } catch (_) {}
    await _pollUpdateStatus();
  }

  Future<void> _downloadAndInstall() async {
    setState(() {
      _downloading = true;
      _updateError = '';
    });
    try {
      final result = await context
          .read<AppState>()
          .api
          .command({'command': 'update_download'});
      if (!mounted) return;
      if (result['downloaded'] == true) {
        await _pollUpdateStatus();
        // 确认后启动安装器（对齐原版 Install 按钮语义）。
        final l10n = AppLocalizations.of(context);
        final confirmed = await showDialog<bool>(
          context: context,
          builder: (dialogContext) => ContentDialog(
            title: Text(l10n.aboutInstallConfirmTitle),
            content: Text(l10n.aboutInstallConfirmBody(_downloadedVersion)),
            actions: [
              Button(
                onPressed: () => Navigator.pop(dialogContext, false),
                child: Text(l10n.cancel),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(dialogContext, true),
                child: Text(l10n.aboutInstallNow),
              ),
            ],
          ),
        );
        if (confirmed == true) {
          await context.read<AppState>().api.command({'command': 'update_install'});
        }
      }
    } catch (e) {
      if (!mounted) return;
      setState(() => _updateError = e.toString());
    } finally {
      if (mounted) setState(() => _downloading = false);
    }
  }

  Future<void> _ignoreVersion() async {
    final version = _latest;
    if (version == null) return;
    await context.read<AppState>().api.command({
      'command': 'update_ignore',
      'version': version,
    });
    await _pollUpdateStatus();
  }

  Future<void> _remindLater() async {
    final version = _latest;
    if (version == null) return;
    await context.read<AppState>().api.command({
      'command': 'update_remind',
      'version': version,
    });
    await _pollUpdateStatus();
  }

  Future<void> _loadVersion() async {
    try {
      final result =
          await context.read<AppState>().api.command({'command': 'app_info'});
      if (!mounted) return;
      setState(() => _version = result['version'] as String? ?? '');
    } catch (_) {
      // 运行时离线时显示未知版本。
    }
  }

  Future<void> _copyVersion() async {
    final l10n = AppLocalizations.of(context);
    final latest = _latest == null || _latest!.isEmpty
        ? l10n.aboutLatestNotChecked
        : _latest!;
    await Clipboard.setData(
        ClipboardData(text: l10n.aboutCopyFormat(_version, latest)));
    if (!mounted) return;
    displayInfoBar(context,
        builder: (ctx, close) =>
            InfoBar(title: Text(l10n.aboutCopied), severity: InfoBarSeverity.success));
  }

  Future<void> _viewLicenses() async {
    final l10n = AppLocalizations.of(context);
    final NeurolingsCE = await rootBundle.loadString(
        'assets/licenses/NeurolingsCE.LICENSE.txt');
    final shijimaQt =
        await rootBundle.loadString('assets/licenses/Shijima-Qt.LICENSE.txt');
    final libshijima =
        await rootBundle.loadString('assets/licenses/libshijima.LICENSE.txt');
    final thirdParty =
        await rootBundle.loadString('assets/licenses/THIRD-PARTY.txt');
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => ContentDialog(
        title: Text(l10n.aboutLicenses),
        content: SizedBox(
          width: 520,
          height: 420,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                _licenseSection('NeurolingsCE', NeurolingsCE),
                _licenseSection('Shijima-Qt', shijimaQt),
                _licenseSection('libshijima', libshijima),
                _licenseSection(l10n.aboutThirdParty, thirdParty),
              ],
            ),
          ),
        ),
        actions: [
          FilledButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(l10n.ok)),
        ],
      ),
    );
  }

  Widget _licenseSection(String title, String text) {
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(title, style: const TextStyle(fontWeight: FontWeight.w600)),
      const SizedBox(height: 4),
      SelectableText(
        text,
        style: const TextStyle(fontFamily: 'Consolas, monospace', fontSize: 11),
      ),
      const SizedBox(height: 16),
    ]);
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.navAbout)),
      children: [
        // 版本卡
        Card(
          margin: const EdgeInsets.symmetric(horizontal: 16),
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(l10n.aboutVersion,
                  style: const TextStyle(fontWeight: FontWeight.w600)),
              const SizedBox(height: 8),
              Text('${l10n.aboutCurrent}: $_version'),
              Text(
                  '${l10n.aboutLatest}: ${_latest == null ? l10n.aboutLatestNotChecked : _latest}'),
              const SizedBox(height: 10),
              Button(onPressed: _copyVersion, child: Text(l10n.aboutCopyVersion)),
            ]),
          ),
        ),
        const SizedBox(height: 8),
        // 更新卡
        Card(
          margin: const EdgeInsets.symmetric(horizontal: 16),
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(l10n.aboutUpdates,
                  style: const TextStyle(fontWeight: FontWeight.w600)),
              const SizedBox(height: 8),
              if (_notify && _latest != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: InfoBar(
                    title: Text(l10n.aboutUpdateAvailable(_latest!)),
                    severity: InfoBarSeverity.info,
                  ),
                ),
              if (_updateError.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Text(_updateError,
                      style: const TextStyle(fontSize: 12)),
                ),
              Wrap(spacing: 8, runSpacing: 8, children: [
                Button(
                  onPressed: _checkForUpdates,
                  child: Text(l10n.aboutCheckForUpdates),
                ),
                if (_latest != null && _downloadedVersion != _latest)
                  FilledButton(
                    onPressed: _downloading ? null : _downloadAndInstall,
                    child: _downloading
                        ? const SizedBox(
                            width: 14,
                            height: 14,
                            child: ProgressRing(strokeWidth: 2))
                        : Text(l10n.aboutDownloadInstall),
                  ),
                if (_downloadedVersion.isNotEmpty)
                  Button(
                    onPressed: _downloadAndInstall,
                    child: Text(l10n.aboutInstall(_downloadedVersion)),
                  ),
                if (_latest != null)
                  Button(onPressed: _ignoreVersion, child: Text(l10n.aboutIgnoreVersion)),
                if (_latest != null)
                  Button(onPressed: _remindLater, child: Text(l10n.aboutRemindLater)),
                Button(
                  onPressed: () => launchUrl(Uri.parse(releasesUrl)),
                  child: Text(l10n.aboutOpenReleasePage),
                ),
                Button(
                  onPressed: () => launchUrl(Uri.parse(releasesUrl)),
                  child: Text(l10n.aboutViewReleaseNotes),
                ),
              ]),
            ]),
          ),
        ),
        const SizedBox(height: 8),
        // 项目卡
        Card(
          margin: const EdgeInsets.symmetric(horizontal: 16),
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(l10n.aboutProject,
                  style: const TextStyle(fontWeight: FontWeight.w600)),
              const SizedBox(height: 8),
              _link(l10n.aboutUpstream, upstreamUrl),
              _link('GitHub', githubUrl),
              Text('${l10n.aboutQQGroup}: $qqGroup'),
              Text('${l10n.license}: GPLv3'),
              const SizedBox(height: 10),
              Wrap(spacing: 8, runSpacing: 8, children: [
                Button(onPressed: _viewLicenses, child: Text(l10n.aboutLicenses)),
                Button(
                    onPressed: () => launchUrl(Uri.parse(issuesUrl)),
                    child: Text(l10n.aboutReportIssue)),
              ]),
            ]),
          ),
        ),
      ],
    );
  }

  Widget _link(String label, String url) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: HyperlinkButton(
        style: ButtonStyle(padding: WidgetStateProperty.all(EdgeInsets.zero)),
        onPressed: () => launchUrl(Uri.parse(url)),
        child: Text('$label: $url'),
      ),
    );
  }
}
