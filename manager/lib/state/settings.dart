import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

/// 语言偏好：读写运行时 settings.json 的 language 键（en/zh_CN）。
/// 未设置时回退系统区域设置（zh* → zh，其余 en，对齐原版启动语言判定）。
class SettingsController extends ChangeNotifier {
  String _locale = _load();
  String get locale => _locale;

  static String get settingsPath {
    final home =
        Platform.environment['USERPROFILE'] ??
        Platform.environment['HOME'] ??
        '';
    if (Platform.isWindows) {
      final local = Platform.environment['LOCALAPPDATA'] ?? home;
      return '$local\\NeurolingsCE\\settings.json';
    }
    if (Platform.isMacOS) {
      return '$home/Library/Application Support/NeurolingsCE/settings.json';
    }
    return '$home/.local/share/NeurolingsCE/settings.json';
  }

  static Map<String, dynamic> _readSettings() {
    try {
      final decoded = jsonDecode(File(settingsPath).readAsStringSync());
      if (decoded is Map<String, dynamic>) return decoded;
    } catch (_) {}
    return {};
  }

  static String _load() {
    final value = _readSettings()['language'];
    if (value is String && value.trim().isNotEmpty) {
      return value.trim().toLowerCase().startsWith('zh') ? 'zh' : 'en';
    }
    // 系统区域回退，并写入 settings.json，让运行时托盘语言与管理器一致。
    final locale = Platform.localeName.toLowerCase().startsWith('zh')
        ? 'zh'
        : 'en';
    _persistLocale(locale);
    return locale;
  }

  void setLocale(String locale) {
    if (locale == _locale) return;
    _locale = locale;
    // 兜底直接落盘（运行时在线时设置页会再经 set_settings 写入同一键）。
    _persistLocale(locale);
    notifyListeners();
  }

  static void _persistLocale(String locale) {
    try {
      final settings = _readSettings();
      settings['language'] = locale == 'zh' ? 'zh_CN' : 'en';
      final file = File(settingsPath);
      final tmp = '$settingsPath.tmp';
      file.parent.createSync(recursive: true);
      File(
        tmp,
      ).writeAsStringSync(const JsonEncoder.withIndent('  ').convert(settings));
      File(tmp).renameSync(settingsPath);
    } catch (_) {
      // 写入失败时下次启动回退系统语言。
    }
  }
}
