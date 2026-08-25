import 'dart:convert';

import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;

import 'package:neurolings_manager/api/runtime_api.dart';
import 'package:neurolings_manager/main.dart';

class _CommandReply {
  final int statusCode;
  final Map<String, dynamic> body;

  const _CommandReply(this.statusCode, this.body);
}

class _QueuedCommandClient extends http.BaseClient {
  final List<_CommandReply> _replies;
  final List<Map<String, dynamic>> requests = [];

  _QueuedCommandClient(List<_CommandReply> replies)
    : _replies = List<_CommandReply>.from(replies);

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async {
    final commandRequest = request as http.Request;
    requests.add(
      (jsonDecode(commandRequest.body) as Map).cast<String, dynamic>(),
    );
    final reply = _replies.removeAt(0);
    return http.StreamedResponse(
      Stream<List<int>>.value(utf8.encode(jsonEncode(reply.body))),
      reply.statusCode,
      headers: const {'content-type': 'application/json'},
    );
  }
}

void main() {
  // 测试环境无平台窗口，mock window_manager 通道。
  setUpAll(() {
    TestWidgetsFlutterBinding.ensureInitialized();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(const MethodChannel('window_manager'), (
          call,
        ) async {
          return null;
        });
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(const MethodChannel('desktop_drop'), (
          call,
        ) async {
          return null;
        });
  });

  test('后台命令轮询最终状态且不重放写请求', () async {
    final client = _QueuedCommandClient(const [
      _CommandReply(202, {
        'accepted': true,
        'pending': true,
        'operation_id': 17,
        'status': 202,
      }),
      _CommandReply(202, {
        'accepted': true,
        'pending': true,
        'operation_id': 17,
        'status': 202,
      }),
      _CommandReply(200, {
        'pending': false,
        'operation_id': 17,
        'operation_state': 'completed',
        'installed': true,
      }),
    ]);
    final api = RuntimeApi(
      client: client,
      controlToken: 'test-token',
      internalPorts: const [32457],
      operationPollInterval: Duration.zero,
      operationCompletionTimeout: const Duration(seconds: 1),
    );

    final result = await api.command({
      'command': 'store_install',
      'id': 'demo',
    });

    expect(result['installed'], true);
    expect(result['operation_state'], 'completed');
    expect(client.requests.map((request) => request['command']).toList(), [
      'store_install',
      'operation_status',
      'operation_status',
    ]);
    api.close();
  });

  test('默认令牌在运行时启动后可供既有客户端读取', () async {
    String? runtimeToken;
    final client = _QueuedCommandClient(const [
      _CommandReply(200, {'saved': true}),
    ]);
    final api = RuntimeApi(
      client: client,
      controlTokenProvider: () => runtimeToken,
      internalPorts: const [32457],
    );

    await expectLater(
      api.command({'command': 'set_settings'}),
      throwsA(
        isA<ApiException>().having((error) => error.status, 'status', 401),
      ),
    );
    expect(client.requests, isEmpty);

    runtimeToken = 'test-token';
    final result = await api.command({'command': 'set_settings'});
    expect(result['saved'], true);
    expect(client.requests.single['command'], 'set_settings');
    api.close();
  });

  test('管理器心跳携带窗口级可见性', () {
    expect(
      buildManagerHeartbeatPayload(
        x: -1280,
        y: 48,
        width: 1000,
        height: 680,
        isVisible: false,
      ),
      {
        'command': 'manager_heartbeat',
        'x': -1280,
        'y': 48,
        'width': 1000,
        'height': 680,
        'is_visible': false,
      },
    );
  });

  test('后台命令最终失败时向调用页透传错误', () async {
    final client = _QueuedCommandClient(const [
      _CommandReply(202, {
        'accepted': true,
        'pending': true,
        'operation_id': 23,
        'status': 202,
      }),
      _CommandReply(422, {
        'error': 'package signature is invalid',
        'status': 422,
        'operation_id': 23,
        'pending': false,
        'operation_state': 'failed',
      }),
    ]);
    final api = RuntimeApi(
      client: client,
      controlToken: 'test-token',
      internalPorts: const [32457],
      operationPollInterval: Duration.zero,
      operationCompletionTimeout: const Duration(seconds: 1),
    );

    await expectLater(
      api.command({
        'command': 'import_mascot_template',
        'archive_path': 'demo.mascot',
      }),
      throwsA(
        isA<ApiException>()
            .having((error) => error.status, 'status', 422)
            .having(
              (error) => error.message,
              'message',
              'package signature is invalid',
            ),
      ),
    );
    expect(client.requests.map((request) => request['command']).toList(), [
      'import_mascot_template',
      'operation_status',
    ]);
    api.close();
  });

  testWidgets('manager shell renders navigation and status bar', (
    tester,
  ) async {
    await tester.pumpWidget(const ManagerApp());
    await tester.pumpAndSettle();

    expect(find.byType(NavigationView), findsOneWidget);
    // 状态栏结构不依赖当前语言，避免把英文文案误当成界面契约。
    expect(find.byKey(const Key('manager-status-counts')), findsOneWidget);
    // 卸载 shell 以取消心跳/轮询定时器。
    await tester.pumpWidget(const SizedBox());
  });
}
