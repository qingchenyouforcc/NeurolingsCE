import 'dart:io';

import 'package:flutter/foundation.dart';

/// User preferences (locale). Persisted to a small JSON file next to the app.
class SettingsController extends ChangeNotifier {
  static const _supported = ['en', 'zh'];

  String _locale = _load();
  String get locale => _locale;

  static String _load() {
    try {
      final file = File(_path());
      if (file.existsSync()) {
        final content = file.readAsStringSync().trim();
        if (_supported.contains(content)) return content;
      }
    } catch (_) {}
    return 'en';
  }

  static String _path() {
    final dir = Platform.resolvedExecutable;
    final base = dir.substring(0, dir.lastIndexOf(Platform.pathSeparator));
    return '$base${Platform.pathSeparator}neurolings_manager_locale.txt';
  }

  void setLocale(String locale) {
    if (!_supported.contains(locale) || locale == _locale) return;
    _locale = locale;
    try {
      File(_path()).writeAsStringSync(locale);
    } catch (_) {}
    notifyListeners();
  }
}
