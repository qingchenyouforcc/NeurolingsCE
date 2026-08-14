import 'package:flutter/foundation.dart';

import '../api/runtime_api.dart';

/// Shared application state: runtime connectivity and mascot lists.
class AppState extends ChangeNotifier {
  final RuntimeApi api = RuntimeApi();

  bool _runtimeOnline = false;
  bool get runtimeOnline => _runtimeOnline;

  bool _busy = false;
  bool get busy => _busy;

  List<LoadedMascot> _templates = [];
  List<LoadedMascot> get templates => _templates;

  List<RunningMascot> _running = [];
  List<RunningMascot> get running => _running;

  String? _lastError;
  String? get lastError => _lastError;

  Future<void> refresh() async {
    _busy = true;
    _lastError = null;
    notifyListeners();
    try {
      _runtimeOnline = await api.ping();
      if (_runtimeOnline) {
        _templates = await api.loadedMascots();
        _running = await api.runningMascots();
      } else {
        _templates = [];
        _running = [];
      }
    } catch (e) {
      _runtimeOnline = false;
      _lastError = e.toString();
    } finally {
      _busy = false;
      notifyListeners();
    }
  }

  Future<void> startRuntime() async {
    _busy = true;
    notifyListeners();
    final started = await startRuntimeProcess();
    if (!started) {
      _lastError = 'Runtime executable not found';
    } else {
      // Give the runtime a moment to come up before probing.
      await Future.delayed(const Duration(milliseconds: 800));
    }
    await refresh();
  }

  Future<void> spawn(String name) async {
    try {
      await api.spawn(name: name);
    } catch (e) {
      _lastError = e.toString();
    }
    await refresh();
  }

  Future<void> dismiss(int id) async {
    try {
      await api.dismiss(id);
    } catch (e) {
      _lastError = e.toString();
    }
    await refresh();
  }

  Future<void> dismissAll() async {
    try {
      await api.dismissAll();
    } catch (e) {
      _lastError = e.toString();
    }
    await refresh();
  }

  /// Imports an archive via the standalone CLI. Returns the CLI output.
  Future<String> importArchive(String path) async {
    _busy = true;
    notifyListeners();
    try {
      final (code, stdout, stderr) =
          await runCli(['--json', '--mascot', 'add', path]);
      await refresh();
      if (code == 0) return stdout.trim();
      return stderr.trim().isEmpty ? stdout.trim() : stderr.trim();
    } finally {
      _busy = false;
      notifyListeners();
    }
  }

  @override
  void dispose() {
    api.close();
    super.dispose();
  }
}
