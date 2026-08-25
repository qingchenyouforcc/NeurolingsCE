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

  int _mascotCount = 0;
  int get mascotCount => _runtimeOnline ? _mascotCount : 0;

  int _templateCount = 0;
  int get templateCount => _runtimeOnline ? _templateCount : 0;

  int? _pendingInspectId;
  int? get pendingInspectId => _pendingInspectId;

  /// 心跳附带的计数与检查器请求。
  void applyHeartbeat(Map<String, dynamic> response) {
    _mascotCount =
        (response['mascot_count'] as num?)?.toInt() ?? _running.length;
    _templateCount =
        (response['template_count'] as num?)?.toInt() ?? _templates.length;
    final inspect = response['inspect_id'];
    if (inspect is num) {
      _pendingInspectId = inspect.toInt();
    }
    notifyListeners();
  }

  void clearInspect() {
    _pendingInspectId = null;
    notifyListeners();
  }

  Future<void> refresh({bool reloadLibrary = false}) async {
    _busy = true;
    _lastError = null;
    notifyListeners();
    try {
      _runtimeOnline = await api.ping();
      if (_runtimeOnline) {
        if (reloadLibrary) {
          // Refresh 会从磁盘重扫 *.mascot，而不是只读启动时的内存表。
          await api.command({'command': 'reload_templates'});
        }
        _templates = await api.loadedMascots();
        _running = await api.runningMascots();
        _templateCount = _templates.length;
        _mascotCount = _running.length;
      } else {
        _templates = [];
        _running = [];
        _templateCount = 0;
        _mascotCount = 0;
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

  /// 经运行时导入（写入存储并立刻登记到内存模板表）。运行时离线时回退 CLI。
  Future<String> importArchive(String path) async {
    _busy = true;
    notifyListeners();
    try {
      _runtimeOnline = await api.ping();
      if (_runtimeOnline) {
        try {
          final result = await api.command({
            'command': 'import_mascot_template',
            'archive_path': path,
          });
          await refresh();
          final loaded = result['loaded_mascots'];
          if (loaded is List && loaded.isNotEmpty) {
            final names = loaded
                .whereType<Map>()
                .map((e) => e['name']?.toString() ?? '')
                .where((n) => n.isNotEmpty)
                .join(', ');
            return names.isEmpty
                ? 'imported ${loaded.length}'
                : 'imported $names';
          }
          return result.toString();
        } catch (e) {
          _lastError = e.toString();
          return e.toString();
        }
      }
      final (code, stdout, stderr) = await runCli([
        '--json',
        '--mascot',
        'add',
        path,
      ]);
      if (code != 0) {
        return stderr.trim().isEmpty ? stdout.trim() : stderr.trim();
      }
      _runtimeOnline = await api.ping();
      if (_runtimeOnline) {
        await refresh(reloadLibrary: true);
      } else {
        await refresh();
      }
      return stdout.trim().isEmpty ? 'imported' : stdout.trim();
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
