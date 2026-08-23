import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:neurolings_manager/main.dart';

void main() {
  // 测试环境无平台窗口，mock window_manager 通道。
  setUpAll(() {
    TestWidgetsFlutterBinding.ensureInitialized();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(const MethodChannel('window_manager'), (call) async {
      return null;
    });
  });

  testWidgets('manager shell renders navigation and status bar', (tester) async {
    await tester.pumpWidget(const ManagerApp());
    await tester.pumpAndSettle();

    expect(find.byType(NavigationView), findsOneWidget);
    // 状态栏固定显示运行/模板计数（对齐原版 ElaStatusBar）。
    expect(find.textContaining('Mascots:'), findsOneWidget);
    // 卸载 shell 以取消心跳/轮询定时器。
    await tester.pumpWidget(const SizedBox());
  });
}
