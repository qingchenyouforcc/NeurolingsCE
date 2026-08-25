import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:http/http.dart' as http;
import 'package:path/path.dart' as p;

const String apiBase = 'http://127.0.0.1:32456/shijima/api/v1';

const int _publicApiPort = 32456;
const int _internalApiPort = 32457;
const String _internalControlTokenEnv = 'NEUROLINGSCE_MANAGER_TOKEN';
const String _internalControlPortEnv = 'NEUROLINGSCE_MANAGER_PORT';
const Duration _commandResponseTimeout = Duration(seconds: 125);
const Duration _defaultOperationPollInterval = Duration(milliseconds: 500);
// 运行时完成结果仅保留 300 秒；等待窗口预留 30 秒给最后一次轮询。
const Duration _maximumOperationCompletionTimeout = Duration(
  minutes: 4,
  seconds: 30,
);
const Duration _defaultOperationCompletionTimeout =
    _maximumOperationCompletionTimeout;

String? _internalControlToken = _readInternalControlToken();

Duration _boundedOperationCompletionTimeout(Duration? requested) {
  final timeout = requested ?? _defaultOperationCompletionTimeout;
  return timeout.inMicroseconds >
          _maximumOperationCompletionTimeout.inMicroseconds
      ? _maximumOperationCompletionTimeout
      : timeout;
}

Duration _minimumDuration(Duration first, Duration second) =>
    first.inMicroseconds <= second.inMicroseconds ? first : second;

/// 验证 runtime 与 Manager 进程间传递的 256 位 URL-safe Base64 令牌。
bool _isValidInternalControlToken(String? value) {
  if (value == null || !RegExp(r'^[A-Za-z0-9_-]{43}$').hasMatch(value)) {
    return false;
  }
  try {
    return base64Url.decode('$value=').length == 32;
  } on FormatException {
    return false;
  }
}

String? _readInternalControlToken() {
  final value = Platform.environment[_internalControlTokenEnv];
  return _isValidInternalControlToken(value) ? value : null;
}

/// 仅在系统提供密码学随机源时生成新的控制面令牌；没有安全随机源则拒绝启动。
String? _generateInternalControlToken() {
  try {
    final random = Random.secure();
    final bytes = List<int>.generate(
      32,
      (_) => random.nextInt(256),
      growable: false,
    );
    return base64UrlEncode(bytes).replaceAll('=', '');
  } on UnsupportedError {
    return null;
  }
}

class _RuntimeEndpoint {
  final int port;
  final bool internal;

  const _RuntimeEndpoint(this.port, {required this.internal});
}

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

/// Runtime 内部控制面客户端。
///
/// 管理命令只走携带内部令牌的私有端口；公开端口仅保留文档化桌宠 API。
class RuntimeApi {
  final http.Client _client;
  final String? _explicitControlToken;
  final String? Function() _controlTokenProvider;
  late final List<int> _internalPorts;
  final Duration _operationPollInterval;
  final Duration _operationCompletionTimeout;
  _RuntimeEndpoint? _activeEndpoint;

  String? get _controlToken => _explicitControlToken ?? _controlTokenProvider();

  /// 创建控制面客户端；显式令牌和令牌读取器仅用于嵌入式调用与回归测试。
  /// 未传入时，每次请求都会读取当前进程内的控制面令牌。
  RuntimeApi({
    http.Client? client,
    String? controlToken,
    String? Function()? controlTokenProvider,
    List<int>? internalPorts,
    Duration? operationPollInterval,
    Duration? operationCompletionTimeout,
  }) : _client = client ?? http.Client(),
       _explicitControlToken = controlToken,
       _controlTokenProvider =
           controlTokenProvider ?? (() => _internalControlToken),
       _operationPollInterval =
           operationPollInterval ?? _defaultOperationPollInterval,
       _operationCompletionTimeout = _boundedOperationCompletionTimeout(
         operationCompletionTimeout,
       ) {
    if (internalPorts != null) {
      _internalPorts = List<int>.unmodifiable(internalPorts);
    } else {
      final envPort = int.tryParse(
        Platform.environment[_internalControlPortEnv] ?? '',
      );
      final ports = <int>[];
      if (envPort != null && envPort > 0 && envPort <= 65535) {
        ports.add(envPort);
      }
      if (!ports.contains(_internalApiPort)) ports.add(_internalApiPort);
      _internalPorts = ports;
    }
  }

  Uri _uri(int port, String path, [Map<String, String>? query]) => Uri.parse(
    'http://127.0.0.1:$port/shijima/api/v1$path',
  ).replace(queryParameters: query);

  Map<String, String> _headersFor(
    _RuntimeEndpoint endpoint, {
    bool jsonBody = false,
  }) {
    final headers = <String, String>{};
    if (jsonBody) headers['Content-Type'] = 'application/json';
    if (endpoint.internal) {
      final token = _controlToken;
      if (token == null) {
        throw ApiException('Internal control credentials are unavailable', 401);
      }
      headers['Authorization'] = 'Bearer $token';
    }
    return headers;
  }

  Future<_RuntimeEndpoint> _endpointForStandardRoutes() async {
    final active = _activeEndpoint;
    if (active != null) return active;
    if (!await ping()) throw ApiException('Runtime is unavailable');
    return _activeEndpoint!;
  }

  Map<String, dynamic> _decode(http.Response response) {
    final body = response.body.trim();
    final dynamic decoded = body.isEmpty ? {} : jsonDecode(body);
    if (decoded is Map<String, dynamic>) {
      // runtime 的若干成功响应恒带 "error" 键（成功时为空串），只有非空才算失败。
      final error = decoded['error'];
      if (response.statusCode >= 400 || (error is String && error.isNotEmpty)) {
        throw ApiException(
          error is String && error.isNotEmpty ? error : 'Request failed',
          (decoded['status'] as num?)?.toInt() ?? response.statusCode,
        );
      }
      return decoded;
    }
    return {};
  }

  Future<bool> ping() async {
    final token = _controlToken;
    if (token != null) {
      for (final port in _internalPorts) {
        try {
          final response = await _client
              .get(
                _uri(port, '/ping'),
                headers: {'Authorization': 'Bearer $token'},
              )
              .timeout(const Duration(milliseconds: 600));
          if (response.statusCode == 200) {
            final decoded = jsonDecode(response.body);
            if (decoded is Map && decoded['ok'] == true) {
              _activeEndpoint = _RuntimeEndpoint(port, internal: true);
              return true;
            }
          }
        } catch (_) {
          // 尝试下一个私有端口候选。
        }
      }
    }
    try {
      final response = await _client
          .get(_uri(_publicApiPort, '/ping'))
          .timeout(const Duration(milliseconds: 600));
      if (response.statusCode == 200) {
        final decoded = jsonDecode(response.body);
        if (decoded is Map && decoded['ok'] == true) {
          _activeEndpoint = const _RuntimeEndpoint(
            _publicApiPort,
            internal: false,
          );
          return true;
        }
      }
    } catch (_) {
      // 公开 API 未启用或运行时尚未启动。
    }
    _activeEndpoint = null;
    return false;
  }

  Future<List<LoadedMascot>> loadedMascots() async {
    final endpoint = await _endpointForStandardRoutes();
    final response = await _client.get(
      _uri(endpoint.port, '/loadedMascots'),
      headers: _headersFor(endpoint),
    );
    final decoded = _decode(response);
    final list = decoded['loaded_mascots'];
    if (list is! List) return [];
    return list
        .whereType<Map<String, dynamic>>()
        .map(LoadedMascot.fromJson)
        .toList();
  }

  Future<List<RunningMascot>> runningMascots() async {
    final endpoint = await _endpointForStandardRoutes();
    final response = await _client.get(
      _uri(endpoint.port, '/mascots'),
      headers: _headersFor(endpoint),
    );
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
    final endpoint = await _endpointForStandardRoutes();
    final response = await _client.post(
      _uri(endpoint.port, '/mascots'),
      headers: _headersFor(endpoint, jsonBody: true),
      body: jsonEncode(payload),
    );
    final decoded = _decode(response);
    final mascot = decoded['mascot'];
    return mascot is Map<String, dynamic>
        ? RunningMascot.fromJson(mascot)
        : null;
  }

  Future<void> dismiss(int id) async {
    final endpoint = await _endpointForStandardRoutes();
    final response = await _client.delete(
      _uri(endpoint.port, '/mascots/$id'),
      headers: _headersFor(endpoint),
    );
    _decode(response);
  }

  Future<void> dismissAll() async {
    final endpoint = await _endpointForStandardRoutes();
    final response = await _client.delete(
      _uri(endpoint.port, '/mascots'),
      headers: _headersFor(endpoint),
    );
    _decode(response);
  }

  Future<void> stop() async {
    final endpoint = await _endpointForStandardRoutes();
    final response = await _client.delete(
      _uri(endpoint.port, '/mascots'),
      headers: _headersFor(endpoint),
    );
    _decode(response);
  }

  /// 向已鉴权的内部控制面发送单次命令请求。
  Future<Map<String, dynamic>> _postInternalCommand(
    _RuntimeEndpoint endpoint,
    Map<String, dynamic> payload, {
    Duration? timeout,
  }) async {
    final response = await _client
        .post(
          _uri(endpoint.port, '/command'),
          headers: _headersFor(endpoint, jsonBody: true),
          body: jsonEncode(payload),
        )
        .timeout(timeout ?? _commandResponseTimeout);
    return _decode(response);
  }

  /// 等待已受理后台操作的最终响应，绝不重放可能已执行的写命令。
  Future<Map<String, dynamic>> _waitForOperation(
    _RuntimeEndpoint endpoint,
    int operationId,
  ) async {
    final deadline = DateTime.now().add(_operationCompletionTimeout);
    while (true) {
      final pollBudget = deadline.difference(DateTime.now());
      if (pollBudget.inMicroseconds <= 0) break;
      await Future<void>.delayed(
        _minimumDuration(_operationPollInterval, pollBudget),
      );
      final requestBudget = deadline.difference(DateTime.now());
      if (requestBudget.inMicroseconds <= 0) break;
      try {
        final status = await _postInternalCommand(
          endpoint,
          {'command': 'operation_status', 'operation_id': operationId},
          timeout: _minimumDuration(_commandResponseTimeout, requestBudget),
        );
        if (status['pending'] != true) return status;
      } on SocketException {
        throw ApiException(
          'Unable to query operation $operationId; it may still be running',
          503,
        );
      } on TimeoutException {
        if (!DateTime.now().isBefore(deadline)) break;
        throw ApiException(
          'Timed out while querying operation $operationId; it may still be running',
          503,
        );
      }
    }
    throw ApiException(
      'Operation $operationId is still running after ${_operationCompletionTimeout.inSeconds} seconds',
      202,
    );
  }

  /// 通过私有控制面发送运行时扩展命令，绝不回退到公开端口。
  Future<Map<String, dynamic>> command(Map<String, dynamic> payload) async {
    if (_controlToken == null) {
      throw ApiException('Internal control credentials are unavailable', 401);
    }
    for (final port in _internalPorts) {
      final endpoint = _RuntimeEndpoint(port, internal: true);
      late final Map<String, dynamic> decoded;
      try {
        decoded = await _postInternalCommand(endpoint, payload);
      } on SocketException {
        // 仅在端口不可达时尝试另一个私有候选，避免重放可能已受理的写命令。
        continue;
      }
      _activeEndpoint = endpoint;
      if (decoded['pending'] == true) {
        final operationId = (decoded['operation_id'] as num?)?.toInt();
        if (operationId == null || operationId <= 0) {
          throw ApiException(
            'Runtime accepted an operation without a valid identifier',
            502,
          );
        }
        return _waitForOperation(endpoint, operationId);
      }
      return decoded;
    }
    throw ApiException('Runtime internal control endpoint is unavailable');
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
    p.normalize(
      p.join(
        p.dirname(Platform.resolvedExecutable),
        '..',
        '..',
        '..',
        'target',
        'debug',
        exeName,
      ),
    ),
    p.normalize(
      p.join(
        p.dirname(Platform.resolvedExecutable),
        '..',
        '..',
        '..',
        'target',
        'release',
        exeName,
      ),
    ),
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
  final token = _internalControlToken ?? _generateInternalControlToken();
  if (token == null) return false;
  try {
    // 管理器拉起运行时必须带 CLI 运行时标记，否则运行时会再拉起第二个管理器。
    await Process.start(
      exe,
      const ['--neurolingsce-cli-runtime'],
      environment: {
        _internalControlTokenEnv: token,
        _internalControlPortEnv: _internalApiPort.toString(),
      },
      mode: ProcessStartMode.detached,
    );
    // 同一 Manager 进程的所有 RuntimeApi 实例都从这份仅内存令牌读取。
    _internalControlToken = token;
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
    return (
      result.exitCode,
      result.stdout.toString(),
      result.stderr.toString(),
    );
  } catch (e) {
    return (-1, '', e.toString());
  }
}
