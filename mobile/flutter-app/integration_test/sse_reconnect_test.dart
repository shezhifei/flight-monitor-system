import 'dart:io';

import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path_provider/path_provider.dart';

import 'package:flight_monitor/bridge/api.dart';
import 'package:flight_monitor/bridge/api/auth.dart';
import 'package:flight_monitor/bridge/api/chat.dart';
import 'package:flight_monitor/bridge/frb_generated.dart';
import 'package:flight_monitor/realtime/sse_payload.dart';

/// SSE 重连：飞行模式 30s → 恢复 → 自动重连，seq 去重。
///
/// 宿主机编排见 `scripts/mobile/run_sse_reconnect.ps1`。
///
/// ```
/// flutter test integration_test/sse_reconnect_test.dart -d emulator-5554
/// ```

const String kBaseUrl = String.fromEnvironment(
  'FMS_TEST_BASE_URL',
  defaultValue: 'http://10.0.2.2:8000',
);
const String kDeviceId = 'sse-reconnect-device';

Future<bool> _isOffline() async {
  final results = await Connectivity().checkConnectivity();
  return results.isEmpty || results.every((r) => r == ConnectivityResult.none);
}

Future<void> _waitConnectivity(
  bool offline, {
  Duration timeout = const Duration(seconds: 90),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    if (await _isOffline() == offline) return;
    await Future.delayed(const Duration(seconds: 2));
  }
  fail('等待网络${offline ? "断开" : "恢复"}超时');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('SSE reconnect after 30s airplane: no duplicate seq',
      (tester) async {
    await RustLib.init();
    final dir = await getApplicationSupportDirectory();
    await initCore(
      baseUrl: kBaseUrl,
      allowCleartext: true,
      dbPath: '${dir.path}${Platform.pathSeparator}fms_sse_reconn.db',
      operatorContextId: kDeviceId,
    );
    await login(username: 'admin', password: 'admin123');

    final sse = notificationsStream().asBroadcastStream();
    var connected = 0;
    var disconnected = 0;
    final chatSeqs = <int>[];
    final sub = sse.listen((update) {
      switch (update) {
        case SseUpdate_State(field0: final state):
          if (state is SseConnectionState_Connected) connected += 1;
          if (state is SseConnectionState_Disconnected) disconnected += 1;
        case SseUpdate_Event(field0: final ev):
          if (ev.event == 'chat_message' ||
              ev.event == 'dispatch_chat_message') {
            final msg = parseSseChatMessage(ev.data);
            if (msg != null) chatSeqs.add(msg.seqNo);
          }
      }
    });
    addTearDown(sub.cancel);

    final firstDeadline = DateTime.now().add(const Duration(seconds: 20));
    while (DateTime.now().isBefore(firstDeadline) && connected < 1) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(connected, greaterThanOrEqualTo(1), reason: '首次应 Connected');
    debugPrint('SSE_FIRST_CONNECTED');

    debugPrint('SSE_READY_FOR_AIRPLANE');
    await _waitConnectivity(true);
    debugPrint('SSE_AIRPLANE_ON');

    await Future<void>.delayed(const Duration(seconds: 30));
    debugPrint('SSE_AIRPLANE_30S_ELAPSED disconnected=$disconnected');

    debugPrint('SSE_READY_FOR_RESTORE');
    await _waitConnectivity(false);
    debugPrint('SSE_AIRPLANE_OFF');

    final reconnDeadline = DateTime.now().add(const Duration(seconds: 30));
    while (DateTime.now().isBefore(reconnDeadline) && connected < 2) {
      await tester.pump(const Duration(milliseconds: 200));
    }
    expect(connected, greaterThanOrEqualTo(2),
        reason: '恢复后 SSE 应再次 Connected (got $connected, disc=$disconnected)');
    debugPrint('SSE_RECONNECTED connected=$connected');

    final groups = await chatGroups(status: 'active', limit: 5, offset: 0);
    expect(groups.items, isNotEmpty);
    final group = groups.items.firstWhere(
      (g) => !g.readOnly,
      orElse: () => groups.items.first,
    );
    final stamp = DateTime.now().toUtc().millisecondsSinceEpoch.toString();
    final sent = await sendChatMessage(
      groupId: group.groupId,
      content: 'p2-reconn $stamp',
      atAll: false,
    );
    debugPrint('SSE_RECONN_SENT seq=${sent.seqNo}');

    final hitDeadline = DateTime.now().add(const Duration(seconds: 5));
    while (DateTime.now().isBefore(hitDeadline) &&
        !chatSeqs.contains(sent.seqNo.toInt())) {
      await tester.pump(const Duration(milliseconds: 50));
    }

    final unique = chatSeqs.toSet();
    debugPrint('SSE_SEQS n=${chatSeqs.length} unique=${unique.length} $chatSeqs');
    expect(unique.length, chatSeqs.length, reason: '重连后 SSE 聊天 seq 不得重复');
    debugPrint('SSE_RECONNECT_OK');
  });
}
