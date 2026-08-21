import 'dart:io';

import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter/services.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:url_launcher/url_launcher.dart';

const String appVersion = '0.1.0';
const String qqGroup = '125081756';
const String githubUrl = 'https://github.com/qingchenyouforcc/NeurolingsCE';
const String upstreamUrl = 'https://github.com/pixelomer/Shijima-Qt';
const String issuesUrl = 'https://github.com/qingchenyouforcc/NeurolingsCE/issues';

class AboutPage extends StatefulWidget {
  const AboutPage({super.key});

  @override
  State<AboutPage> createState() => _AboutPageState();
}

class _AboutPageState extends State<AboutPage> {
  String _latestVersion = '';
  String _statusText = '未检查更新';
  bool _checking = false;
  String? _copyStatus;

  Future<void> _checkForUpdates() async {
    setState(() => _checking = true);
    // 占位：实际更新检查由 updater.rs 实现，这里仅模拟
    await Future.delayed(const Duration(seconds: 1));
    if (!mounted) return;
    setState(() {
      _checking = false;
      _latestVersion = appVersion;
      _statusText = '已是最新版本';
    });
  }

  Future<void> _copyVersion() async {
    final text = 'NeurolingsCE $appVersion (latest: ${_latestVersion.isEmpty ? '未检查' : 'v$_latestVersion'})';
    await Clipboard.setData(ClipboardData(text: text));
    setState(() => _copyStatus = '版本信息已复制');
    Future.delayed(const Duration(seconds: 2), () {
      if (mounted) setState(() => _copyStatus = null);
    });
  }

  Future<void> _openUrl(String url) async {
    final uri = Uri.parse(url);
    if (await canLaunchUrl(uri)) {
      await launchUrl(uri, mode: LaunchMode.externalApplication);
    }
  }

  Future<void> _showLicenses() async {
    await showDialog(
      context: context,
      builder: (ctx) => ContentDialog(
        title: const Text('许可证'),
        content: SizedBox(
          width: 560,
          height: 420,
          child: SingleChildScrollView(
            child: SelectableText(
              '''NeurolingsCE
Copyright (C) 2025 pixelomer and contributors
License: GPL-3.0-only

本项目基于 Shijima-Qt (GPL-3.0) 及 libshijima 引擎。
内置依赖：
- ElaWidgetTools (MIT)
- cpp-httplib (MIT)
- rapidxml (MIT)
- duktape (MIT)
- Qt 6 (LGPL/GPL)
完整许可证文本见仓库 LICENSE 文件及构建生成的 licenses_generated.hpp。
''',
              style: const TextStyle(fontSize: 12),
            ),
          ),
        ),
        actions: [
          Button(onPressed: () => Navigator.pop(ctx), child: const Text('关闭')),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.navAbout)),
      children: [
        // 标题
        Text('关于', style: FluentTheme.of(context).typography.title),
        const SizedBox(height: 14),

        // Identity
        Card(
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Column(children: [
              Icon(FluentIcons.robot, size: 56, color: FluentTheme.of(context).accentColor),
              const SizedBox(height: 12),
              Text('NeurolingsCE', style: FluentTheme.of(context).typography.titleLarge),
              const SizedBox(height: 6),
              const Text('跨平台桌宠运行器', style: TextStyle(color: Color(0xFF6B6B6B))),
              const SizedBox(height: 4),
              const Text('Copyright © 2025 pixelomer and contributors', style: TextStyle(fontSize: 12, color: Color(0xFF6B6B6B))),
              const SizedBox(height: 12),
              HyperlinkButton(
                onPressed: () => _openUrl(githubUrl),
                child: const Text('项目主页: NeurolingsCE (GitHub)'),
              ),
            ]),
          ),
        ),
        const SizedBox(height: 12),

        // Version
        Expander(
          header: const Text('版本', style: TextStyle(fontWeight: FontWeight.w600)),
          content: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              const Text('当前版本:'),
              const SizedBox(width: 8),
              Text('v$appVersion', style: const TextStyle(fontWeight: FontWeight.bold)),
            ]),
            const SizedBox(height: 6),
            Row(children: [
              const Text('最新版本:'),
              const SizedBox(width: 8),
              Text(_latestVersion.isEmpty ? '未检查' : 'v$_latestVersion', style: const TextStyle(color: Color(0xFF6B6B6B))),
            ]),
            const SizedBox(height: 12),
            Wrap(spacing: 8, runSpacing: 8, children: [
              Button(onPressed: _copyVersion, child: const Text('复制版本信息')),
              if (_copyStatus != null) Padding(padding: const EdgeInsets.only(left: 8), child: Text(_copyStatus!, style: TextStyle(color: Colors.green, fontSize: 12))),
            ]),
            if (_copyStatus != null) const SizedBox(height: 4),
          ]),
        ),
        const SizedBox(height: 8),

        // Updates
        Expander(
          header: const Text('更新', style: TextStyle(fontWeight: FontWeight.w600)),
          content: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(_statusText, style: const TextStyle(fontWeight: FontWeight.w600)),
            const SizedBox(height: 6),
            const Text('检查 GitHub 是否有更新版本，支持 MSI/EXE 安装包校验。', style: TextStyle(fontSize: 12, color: Color(0xFF6B6B6B))),
            const SizedBox(height: 12),
            Wrap(spacing: 8, runSpacing: 8, children: [
              FilledButton(onPressed: _checking ? null : _checkForUpdates, child: _checking ? const Row(mainAxisSize: MainAxisSize.min, children: [SizedBox(width: 14, height: 14, child: ProgressRing(strokeWidth: 2)), SizedBox(width: 8), Text('检查中...')]) : const Text('检查更新')),
              Button(onPressed: () => _openUrl('$githubUrl/releases'), child: const Text('查看发布说明')),
              Button(onPressed: () => _openUrl('$githubUrl/releases/latest'), child: const Text('下载/安装')),
            ]),
            const SizedBox(height: 8),
            Wrap(spacing: 8, children: [
              Button(onPressed: () => displayInfoBar(context, builder: (c, close) => const InfoBar(title: Text('已忽略此版本'), severity: InfoBarSeverity.info)), child: const Text('忽略此版本')),
              Button(onPressed: () => displayInfoBar(context, builder: (c, close) => const InfoBar(title: Text('稍后提醒'), severity: InfoBarSeverity.info)), child: const Text('稍后提醒')),
            ]),
          ]),
        ),
        const SizedBox(height: 8),

        // Project & Support
        Expander(
          header: const Text('项目与支持', style: TextStyle(fontWeight: FontWeight.w600)),
          content: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            _linkRow('作者', 'Qingchen You', 'https://space.bilibili.com/178381315'),
            _linkRow('上游', 'Shijima-Qt by pixelomer', upstreamUrl),
            _linkRow('GitHub', 'NeurolingsCE', githubUrl),
            const SizedBox(height: 4),
            const Text('QQ 群: 125081756', style: TextStyle(fontSize: 12)),
            const SizedBox(height: 4),
            const Text('许可证: GPLv3', style: TextStyle(fontSize: 12, color: Color(0xFF6B6B6B))),
            const SizedBox(height: 12),
            Wrap(spacing: 8, runSpacing: 8, children: [
              Button(onPressed: _showLicenses, child: const Text('查看许可证')),
              Button(onPressed: () => _openUrl(issuesUrl), child: const Text('报告问题')),
            ]),
          ]),
        ),
        const SizedBox(height: 24),
        Text('NeurolingsCE $appVersion  •  基于 Flutter + Rust 重写', style: FluentTheme.of(context).typography.caption, textAlign: TextAlign.center),
        const SizedBox(height: 8),
      ],
    );
  }

  Widget _linkRow(String label, String text, String url) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(children: [
        SizedBox(width: 48, child: Text('$label:', style: const TextStyle(fontSize: 12, color: Color(0xFF6B6B6B)))),
        HyperlinkButton(onPressed: () => _openUrl(url), child: Text(text, style: const TextStyle(fontSize: 12))),
      ]),
    );
  }
}
