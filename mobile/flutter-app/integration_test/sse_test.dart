import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:flight_monitor/bridge/api.dart';
import 'package:flight_monitor/bridge/api/session.dart';
import 'package:flight_monitor/bridge/frb_generated.dart';

/// P0 task 5 acceptance: connect the notifications SSE stream from a real
/// device/emulator to the local backend. The token is injected manually via
/// `--dart-define=FMS_TEST_TOKEN=...`; since P1 the stream reads the session
/// from the initialized runtime, so the token is restored there first.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('notifications stream connects and receives initial payload',
      (tester) async {
    const token = String.fromEnvironment('FMS_TEST_TOKEN');
    const baseUrl =
        String.fromEnvironment('FMS_TEST_BASE_URL',
            defaultValue: 'http://10.0.2.2:8000');
    if (token.isEmpty) {
      // Token not injected — skip instead of failing (CI has no backend).
      return;
    }

    await RustLib.init();
    await initCore(
      baseUrl: baseUrl,
      allowCleartext: true,
      dbPath:
          '${Directory.systemTemp.path}${Platform.pathSeparator}fms_it_offline.db',
      operatorContextId: 'integration-test-device',
    );
    await restoreTokens(
      bundle: TokenBundle(
        accessToken: token,
        refreshToken: 'unused',
        sessionSecret: 'unused',
        accessExpireAt: DateTime.now().millisecondsSinceEpoch ~/ 1000 + 3600,
      ),
    );

    final events = notificationsStream();

    var connected = false;
    SseUpdate_Event? firstEvent;
    await for (final update in events.timeout(
      const Duration(seconds: 30),
      onTimeout: (sink) => sink.close(),
    )) {
      switch (update) {
        case SseUpdate_State(field0: final state):
          if (state is SseConnectionState_Connected) connected = true;
          if (state is SseConnectionState_Disconnected) {
            fail('SSE disconnected before any event: ${state.reason}');
          }
        case SseUpdate_Event():
          firstEvent = update;
      }
      if (connected && firstEvent != null) break;
    }

    expect(connected, isTrue, reason: 'never reached Connected state');
    expect(firstEvent, isNotNull,
        reason: 'no SSE event (expected initial payload) within 30s');
  });
}
