import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path_provider/path_provider.dart';

import 'package:flight_monitor/bridge/api.dart';
import 'package:flight_monitor/bridge/api/auth.dart';
import 'package:flight_monitor/bridge/api/business_case.dart';
import 'package:flight_monitor/bridge/api/chat.dart';
import 'package:flight_monitor/bridge/api/dispatch.dart';
import 'package:flight_monitor/bridge/api/handover.dart';
import 'package:flight_monitor/bridge/api/notification.dart' as notif_api;
import 'package:flight_monitor/bridge/api/operations.dart';
import 'package:flight_monitor/bridge/api/session.dart';
import 'package:flight_monitor/bridge/frb_generated.dart';

/// 模拟器冒烟：登录后逐域打读路径（派工、协作、事项、战情）。
///
/// ```
/// flutter test integration_test/domain_reads_smoke_test.dart -d emulator-5554 \
///   --dart-define=FMS_TEST_BASE_URL=http://10.0.2.2:8000
/// ```

const String kBaseUrl = String.fromEnvironment(
  'FMS_TEST_BASE_URL',
  defaultValue: 'http://10.0.2.2:8000',
);
const String kDeviceId = 'domain-reads-smoke-device';

bool _rustReady = false;

Future<void> ensureRust() async {
  if (_rustReady) return;
  await RustLib.init();
  _rustReady = true;
}

Future<void> initCoreForTest() async {
  final dir = await getApplicationSupportDirectory();
  await initCore(
    baseUrl: kBaseUrl,
    allowCleartext: true,
    dbPath: '${dir.path}${Platform.pathSeparator}fms_offline_smoke.db',
    operatorContextId: kDeviceId,
  );
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('login + domain reads all 200-class', (tester) async {
    await ensureRust();
    await initCoreForTest();

    // --- Auth ---
    await login(username: 'admin', password: 'admin123');
    final bundle = await currentTokenBundle();
    expect(bundle, isNotNull);
    expect(bundle!.sessionSecret, isNotEmpty);
    debugPrint('SMOKE_LOGIN_OK secret_len=${bundle.sessionSecret.length}');

    // --- workbench / dispatch ---
    final wb = await workbench(pendingSyncCount: 0, maxOrders: 20);
    expect(wb.userId, isNotEmpty);
    debugPrint('SMOKE_WORKBENCH orders=${wb.myOrders.length} '
        'notif=${wb.notificationUnreadCount} chat=${wb.chatUnreadTotal} '
        'handover=${wb.pendingShiftHandoverCount}');

    final orders = await myAssignedOrders();
    debugPrint('SMOKE_MY_ASSIGNED count=${orders.length}');
    expect(orders, isNotEmpty);

    // --- chat ---
    final groups = await chatGroups(status: 'active', limit: 10, offset: 0);
    debugPrint('SMOKE_CHAT_GROUPS total=${groups.total} items=${groups.items.length} '
        'unread_total=${groups.unreadTotal}');
    // admin 探针环境有群；空列表也算通（路由 200）。
    expect(groups.items, isNotNull);
    if (groups.items.isNotEmpty) {
      final g = groups.items.first;
      final msgs = await chatMessages(groupId: g.groupId, limit: 20);
      debugPrint('SMOKE_CHAT_MSGS group=${g.groupId} count=${msgs.items.length}');
    }

    // --- notifications ---
    final notifs = await notif_api.notifications(
      limit: 20,
      offset: 0,
      onlyUnread: false,
    );
    final unread = await notif_api.unreadCount();
    debugPrint('SMOKE_NOTIFS total=${notifs.total} items=${notifs.items.length} '
        'unread=$unread');

    // --- handover ---
    final handovers = await shiftHandovers(status: null, limit: 20, offset: 0);
    debugPrint('SMOKE_HANDOVERS count=${handovers.length}');
    if (handovers.isNotEmpty) {
      final h = await shiftHandoverDetail(id: handovers.first.handoverId);
      debugPrint('SMOKE_HANDOVER_DETAIL id=${h.handoverId} items=${h.items.length}');
    }

    // --- business cases ---
    final cases = await businessCases();
    debugPrint('SMOKE_BUSINESS_CASES count=${cases.length}');
    if (cases.isNotEmpty) {
      final c = await businessCaseDetail(id: cases.first.caseId);
      debugPrint('SMOKE_CASE_DETAIL id=${c.caseId} status=${c.status} '
          'appends=${c.appendCount}');
    }
    final types = await businessCaseTypes(activeOnly: true);
    debugPrint('SMOKE_CASE_TYPES count=${types.length}');
    expect(types, isNotEmpty, reason: '应有活跃事项类型');

    // --- operations ---
    final ops = await operationsEvents(limit: 20);
    debugPrint('SMOKE_OPS events=${ops.events.length} '
        'type_counts=${ops.eventTypeCounts.length}');
    expect(ops.events, isNotEmpty, reason: '战情事件流应有数据');

    // --- auth heartbeat (raw ack path) ---
    await authHeartbeat();
    debugPrint('SMOKE_AUTH_HEARTBEAT_OK');

    // --- SSE: open stream briefly, expect connecting/connected state ---
    final sse = notificationsStream();
    final first = await sse
        .timeout(const Duration(seconds: 15))
        .first
        .timeout(const Duration(seconds: 20));
    debugPrint('SMOKE_SSE_FIRST=$first');
    expect(first, isNotNull);

    debugPrint('SMOKE_ALL_DONE');
  });
}
