import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:neurolings_manager/main.dart';

void main() {
  testWidgets('manager shell renders navigation and home page', (tester) async {
    await tester.pumpWidget(const ManagerApp());
    await tester.pumpAndSettle();

    expect(find.byType(NavigationView), findsOneWidget);
    // Home page shows the runtime status card (runtime is offline in tests).
    expect(find.textContaining('Runtime'), findsWidgets);
  });
}
