import 'dart:convert';
import 'dart:io';

import 'package:http/http.dart' as http;
import 'package:path/path.dart' as p;

const String apiBase = 'http://127.0.0.1:32456/shijima/api/v1';

class LoadedMascot {
  final int id;
  final String name;
  final String version;
  final String description;
  final String author;

  LoadedMascot({
    required this.id,
    required this.name,
    required this.version,
    required this.description,
    required this.author,
  });

  factory LoadedMascot.fromJson(Map<String, dynamic> json) => LoadedMascot(
        id: (json['id'] as num?)?.toInt() ?? 0,
        name: (json['name'] as String?) ?? '',
        version: (json['version'] as String?) ?? '',
        description: (json['description'] as String?) ?? '',
        author: (json['author'] as String?) ?? '',
      );
}

class RunningMascot {
  final int id;
  final int dataId;
  final String name;
  final int? label;
  final double anchorX;
  final double anchorY;
  final String? activeBehavior;

  RunningMascot({
    required this.id,
    required this.dataId,
    required this.name,
    required this.label,
    required this.anchorX,
    required this.anchorY,
    required this.activeBehavior,
  });

  factory RunningMascot.fromJson(Map<String, dynamic> json) {
    final anchor = json['anchor'];
    return RunningMascot(
      id: (json['id'] as num?)?.toInt() ?? 0,
      dataId: (json['data_id'] as num?)?.toInt() ?? 0,
      name: (json['name'] as String?) ?? '',
      label: (json['label'] as num?)?.toInt(),
      anchorX: anchor is Map ? ((anchor['x'] as num?)?.toDouble() ?? 0) : 0,
      anchorY: anchor is Map ? ((anchor['y'] as num?)?.toDouble() ?? 0) : 0,
      activeBehavior: json['active_behavior'] as String?,
    );
  }
}

class ApiException implements Exception {
  final String message;
  final int? status;
  ApiException(this.message, [this.status]);
  @override
  String toString() => status == null ? message : '$message (status $status)';
}

/// Client for the runtime HTTP API (docs/HTTP-API.md).
class RuntimeApi {
  final http.Client _client = http.Client();

  Uri _uri(String path, [Map<String, String>? query]) =>
      Uri.parse('$apiBase$path').replace(queryParameters: query);

  Map<String, dynamic> _decode(http.Response response) {
    final body = response.body.trim();
    final dynamic decoded = body.isEmpty ? {} : jsonDecode(body);
    if (decoded is Map<String, dynamic>) {
      if (response.statusCode >= 400 || decoded.containsKey('error')) {
        throw ApiException(
          (decoded['error'] as String?) ?? 'Request failed',
          (decoded['status'] as num?)?.toInt() ?? response.statusCode,
        );
      }
      return decoded;
    }
    return {};
  }

  Future<bool> ping() async {
    try {
      final response =
          await _client.get(_uri('/ping')).timeout(const Duration(milliseconds: 600));
      if (response.statusCode != 200) return false;
      final decoded = jsonDecode(response.body);
      return decoded is Map && decoded['ok'] == true;
    } catch (_) {
      return false;
    }
  }

  Future<List<LoadedMascot>> loadedMascots() async {
    final response = await _client.get(_uri('/loadedMascots'));
    final decoded = _decode(response);
    final list = decoded['loaded_mascots'];
    if (list is! List) return [];
    return list
        .whereType<Map<String, dynamic>>()
        .map(LoadedMascot.fromJson)
        .toList();
  }

  Future<List<RunningMascot>> runningMascots() async {
    final response = await _client.get(_uri('/mascots'));
    final decoded = _decode(response);
    final list = decoded['mascots'];
    if (list is! List) return [];
    return list
        .whereType<Map<String, dynamic>>()
        .map(RunningMascot.fromJson)
        .toList();
  }

  Future<RunningMascot?> spawn({String? name, int? dataId}) async {
    final payload = <String, dynamic>{};
    if (name != null) payload['name'] = name;
    if (dataId != null) payload['data_id'] = dataId;
    final response = await _client.post(
      _uri('/mascots'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode(payload),
    );
    final decoded = _decode(response);
    final mascot = decoded['mascot'];
    return mascot is Map<String, dynamic> ? RunningMascot.fromJson(mascot) : null;
  }

  Future<void> dismiss(int id) async {
    final response = await _client.delete(_uri('/mascots/$id'));
    _decode(response);
  }

  Future<void> dismissAll() async {
    final response = await _client.delete(_uri('/mascots'));
    _decode(response);
  }

  Future<void> stop() async {
    final response = await _client.delete(_uri('/mascots'));
    _decode(response);
  }

  void close() => _client.close();
}

/// Locates the runtime / CLI executables: first beside this app, then in the
/// workspace target directory (development fallback).
String? findExecutable(String baseName) {
  final exeName = Platform.isWindows ? '$baseName.exe' : baseName;
  final candidates = <String>[
    p.join(p.dirname(Platform.resolvedExecutable), exeName),
    p.join(p.dirname(Platform.resolvedExecutable), '..', exeName),
    p.normalize(p.join(p.dirname(Platform.resolvedExecutable), '..', '..', '..',
        'target', 'debug', exeName)),
    p.normalize(p.join(p.dirname(Platform.resolvedExecutable), '..', '..', '..',
        'target', 'release', exeName)),
  ];
  for (final candidate in candidates) {
    if (File(candidate).existsSync()) return candidate;
  }
  return null;
}

/// Starts the desktop runtime detached from this process.
Future<bool> startRuntimeProcess() async {
  final exe = findExecutable('NeurolingsCE');
  if (exe == null) return false;
  try {
    await Process.start(exe, const [], mode: ProcessStartMode.detached);
    return true;
  } catch (_) {
    return false;
  }
}

/// Runs NeurolingsCE-cli with the given arguments, returning stdout.
Future<(int, String, String)> runCli(List<String> args) async {
  final exe = findExecutable('NeurolingsCE-cli');
  if (exe == null) {
    return (-1, '', 'NeurolingsCE-cli executable not found');
  }
  try {
    final result = await Process.run(exe, args);
    return (result.exitCode, result.stdout.toString(), result.stderr.toString());
  } catch (e) {
    return (-1, '', e.toString());
  }
}
