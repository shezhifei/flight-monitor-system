import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flight_monitor/main.dart';

void main() {
  testWidgets('bridge smoke: demo screen renders with sign button',
      (WidgetTester tester) async {
    // RustLib.init() runs in main(); the screen widget itself must build
    // without the native library loaded.
    await tester.pumpWidget(const FlightMonitorApp());

    expect(find.text('FFI 签名 Demo'), findsOneWidget);
    expect(find.text('运行 ping_sign_demo'), findsOneWidget);
    expect(find.byType(FilledButton), findsOneWidget);
  });
}
