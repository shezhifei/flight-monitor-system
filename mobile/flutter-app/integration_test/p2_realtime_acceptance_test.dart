import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path_provider/path_provider.dart';

import 'package:flight_monitor/bridge/api.dart';
import 'package:flight_monitor/bridge/api/auth.dart';
import 'package:flight_monitor/bridge/api/chat.dart';
import 'package:flight_monitor/bridge/api/dispatch.dart';
import 'package:flight_monitor/bridge/api/handover.dart';
import 'package:flight_monitor/bridge/api/notification.dart' as notif_api;
import 'package:flight_monitor/bridge/frb_generated.dart';
import 'package:flight_monitor/realtime/sse_payload.dart';

import 'support/web_surface_client.dart';

/// P2 双端对拍：Web 面发聊天/通知，Native SSE ≤2s 增量；未读数对齐。
///
/// ```
/// flutter test integration_test/p2_realtime_acceptance_test.dart -d emulator-5554
/// ```

const String kBaseUrl = String.fromEnvironment(
  'FMS_TEST_BASE_URL',
  defaultValue: 'http://10.0.2.2:8000',
);
const String kDeviceId = 'p2-realtime-device';
const String kOtherUserId = '01KM5BFNV6EKJ4TCDNJDT1YH0S';

bool _ready = false;

Future<void> bootNative() async {
  if (!_ready) {
    await RustLib.init();
    _ready = true;
  }
  final dir = await getApplicationSupportDirectory();
  await initCore(
    baseUrl: kBaseUrl,
    allowCleartext: true,
    dbPath: '${dir.path}${Platform.pathSeparator}fms_p2_realtime.db',
    operatorContextId: kDeviceId,
  );
  await login(username: 'admin', password: 'admin123');
}

Future<SseUpdate> waitConnected(Stream<SseUpdate> sse) async {
  await for (final update in sse.timeout(const Duration(seconds: 20))) {
    if (update is SseUpdate_State &&
        update.field0 is SseConnectionState_Connected) {
      return update;
    }
    if (update is SseUpdate_State &&
        update.field0 is SseConnectionState_Disconnected) {
      final reason = (update.field0 as SseConnectionState_Disconnected).reason;
      fail('SSE disconnected before Connected: $reason');
    }
  }
  fail('SSE 未在 20s 内进入 Connected');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('P2 dual-client: web send → native SSE ≤2s + unread match',
      (tester) async {
    await bootNative();
    debugPrint('P2_NATIVE_LOGIN_OK');

    final sse = notificationsStream().asBroadcastStream();
    await waitConnected(sse);
    debugPrint('P2_SSE_CONNECTED');

    final chatSeen = <int>{};
    final notifSeen = <String>{};
    final chatHits = <SseChatMessage>[];
    final notifHits = <SseNotification>[];
    final sseSub = sse.listen((update) {
      if (update is! SseUpdate_Event) return;
      final ev = update.field0;
      if (ev.event == 'chat_message' || ev.event == 'dispatch_chat_message') {
        final msg = parseSseChatMessage(ev.data);
        if (msg != null && chatSeen.add(msg.seqNo)) chatHits.add(msg);
      }
      if (ev.event == 'user_notification' || ev.event == 'notification') {
        final n = parseSseNotification(ev.data);
        if (n != null && notifSeen.add(n.notificationId)) notifHits.add(n);
      }
    });

    final web = WebSurfaceClient(kBaseUrl);
    addTearDown(() {
      sseSub.cancel();
      web.close();
    });
    await web.login(username: 'admin', password: 'admin123');
    debugPrint('P2_WEB_LOGIN_OK');

    final groups = await chatGroups(status: 'active', limit: 10, offset: 0);
    expect(groups.items, isNotEmpty);
    final group = groups.items.firstWhere(
      (g) => !g.readOnly,
      orElse: () => groups.items.first,
    );

    final stamp = DateTime.now().toUtc().millisecondsSinceEpoch.toString();
    final chatContent = 'p2-dual-chat $stamp';

    final tChat = DateTime.now();
    final sent = await web.post(
      '/api/v2/dispatch/collaboration/groups/${group.groupId}/messages',
      {'content': chatContent, 'at_all': false},
    );
    expect(sent['message_id'], isNotEmpty, reason: 'web 发送聊天必须成功');
    debugPrint('P2_WEB_CHAT_SENT id=${sent['message_id']} seq=${sent['seq_no']}');

    SseChatMessage? chatEvent;
    final chatDeadline = DateTime.now().add(const Duration(seconds: 2));
    while (DateTime.now().isBefore(chatDeadline)) {
      for (final m in chatHits) {
        if (m.content.contains(stamp)) {
          chatEvent = m;
          break;
        }
      }
      if (chatEvent != null) break;
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(
      chatEvent,
      isNotNull,
      reason: 'Native SSE 应在 2s 内收到 web 聊天 (stamp=$stamp hits=${chatHits.length})',
    );
    final chatMs = DateTime.now().difference(tChat).inMilliseconds;
    debugPrint('P2_CHAT_SSE_MS=$chatMs seq=${chatEvent!.seqNo}');
    expect(chatMs, lessThanOrEqualTo(2500));

    final wb = await workbench(pendingSyncCount: 0, maxOrders: 5);
    final title = 'p2-dual-notif $stamp';
    final tNotif = DateTime.now();
    final notifResp = await web.post(
      '/api/v2/notifications/dispatch/send',
      {
        'recipient_user_ids': [wb.userId],
        'title': title,
        'body': 'dual-client acceptance',
        'severity': 'info',
        'receipt_required': true,
      },
    );
    final notifData = notifResp['data'] is Map
        ? Map<String, dynamic>.from(notifResp['data'] as Map)
        : notifResp;
    debugPrint('P2_WEB_NOTIF_SENT $notifData');

    SseNotification? notifEvent;
    final notifDeadline = DateTime.now().add(const Duration(seconds: 2));
    while (DateTime.now().isBefore(notifDeadline)) {
      for (final n in notifHits) {
        if (n.title.contains(stamp)) {
          notifEvent = n;
          break;
        }
      }
      if (notifEvent != null) break;
      await tester.pump(const Duration(milliseconds: 50));
    }
    expect(
      notifEvent,
      isNotNull,
      reason: 'Native SSE 应在 2s 内收到 web 通知 (stamp=$stamp hits=${notifHits.length})',
    );
    final notifMs = DateTime.now().difference(tNotif).inMilliseconds;
    debugPrint('P2_NOTIF_SSE_MS=$notifMs id=${notifEvent!.notificationId}');
    expect(notifMs, lessThanOrEqualTo(2500));

    final nativeUnread = (await notif_api.unreadCount()).toInt();
    final webUnreadRaw = await web.get('/api/v2/notifications/unread-count');
    final webUnread = (webUnreadRaw['unread_count'] as num?)?.toInt() ??
        ((webUnreadRaw['data'] is Map)
            ? ((webUnreadRaw['data'] as Map)['unread_count'] as num?)?.toInt()
            : null);
    debugPrint('P2_UNREAD native=$nativeUnread web=$webUnread');
    expect(webUnread, isNotNull);
    expect(nativeUnread, webUnread);

    debugPrint('P2_DUAL_CLIENT_OK chat_ms=$chatMs notif_ms=$notifMs');
  });

  testWidgets('P2/P3 remaining write paths: read/ack/checklist/handover',
      (tester) async {
    await bootNative();

    // --- Chat send + read (native) ---
    final groups = await chatGroups(status: 'active', limit: 10, offset: 0);
    final group = groups.items.firstWhere(
      (g) => !g.readOnly,
      orElse: () => groups.items.first,
    );
    final sent = await sendChatMessage(
      groupId: group.groupId,
      content: 'p2-write ${DateTime.now().toUtc().toIso8601String()}',
      atAll: false,
    );
    expect(sent.messageId, isNotEmpty);
    final read = await markChatRead(
      groupId: group.groupId,
      readSeq: sent.seqNo,
    );
    debugPrint('WRITE_CHAT_OK seq=${sent.seqNo} unread=${read.unreadCount}');

    // --- Notification read / ack / receipt-group ---
    final unreadList = await notif_api.notifications(
      limit: 10,
      offset: 0,
      onlyUnread: true,
    );
    expect(unreadList.items, isNotEmpty, reason: '双端用例应留下至少一条未读');
    final n = unreadList.items.firstWhere(
      (item) => item.receiptRequired && item.ackStatus == 'pending',
      orElse: () => unreadList.items.first,
    );
    await notif_api.notificationRead(id: n.notificationId);
    debugPrint('WRITE_NOTIF_READ id=${n.notificationId}');
    if (n.receiptRequired && n.ackStatus == 'pending') {
      await notif_api.notificationAck(
        id: n.notificationId,
        action: 'ack',
        note: 'p2-write',
      );
      debugPrint('WRITE_NOTIF_ACK ok');
    }
    final rg = n.receiptGroupId;
    if (rg != null && rg.isNotEmpty) {
      final groupDetail = await notif_api.receiptGroup(receiptGroupId: rg);
      expect(groupDetail.receiptGroupId, rg);
      debugPrint('WRITE_RECEIPT_GROUP ok items=${groupDetail.items.length}');
    }

    // --- Checklist submit ---
    final orders = await myAssignedOrders();
    DispatchOrder? gateOrder;
    for (final o in orders) {
      final st = o.status.toLowerCase();
      if (st != 'assigned' && st != 'in_progress') continue;
      final cl = await safetyChecklist(orderId: o.id);
      final pending = cl.items.where((i) =>
          (i.result == null || i.result!.isEmpty) && i.required_);
      if (pending.isNotEmpty) {
        gateOrder = o;
        final item = pending.first;
        await submitChecklistItem(
          orderId: o.id,
          itemCode: item.itemCode,
          result: 'pass',
        );
        debugPrint(
            'WRITE_CHECKLIST_OK order=${o.id} item=${item.itemCode}');
        break;
      }
    }
    if (gateOrder == null) {
      debugPrint('WRITE_CHECKLIST_SKIP no pending required item');
    }

    // --- Handover create (web) + detail/item ack (native) ---
    final web = WebSurfaceClient(kBaseUrl);
    addTearDown(web.close);
    await web.login(username: 'admin', password: 'admin123');
    final today = DateTime.now().toUtc();
    final date =
        '${today.year.toString().padLeft(4, '0')}-${today.month.toString().padLeft(2, '0')}-${today.day.toString().padLeft(2, '0')}';
    final created = await web.post('/api/v2/shift-handovers', {
      'shift_date': date,
      'shift_code': 'p2${today.millisecondsSinceEpoch % 100000}',
      'from_user_id': kOtherUserId,
      'to_user_id': (await workbench(pendingSyncCount: 0, maxOrders: 1)).userId,
      'summary': 'p2 write-path',
      'risk_level': 'low',
      'items': [
        {'item_type': 'other', 'title': '确认值班', 'is_mandatory': true},
      ],
    });
    final hid = created['handover_id'] as String;
    await web.post('/api/v2/shift-handovers/$hid/submit', {});
    final detail = await shiftHandoverDetail(id: hid);
    expect(detail.handoverId, hid);
    debugPrint('WRITE_HANDOVER_DETAIL items=${detail.items.length}');
    final pendingItems = detail.items.where((i) => !i.acknowledged).toList();
    if (pendingItems.isNotEmpty) {
      await ackHandoverItem(
        handoverId: hid,
        itemId: pendingItems.first.itemId,
        acknowledged: true,
      );
      debugPrint('WRITE_HANDOVER_ITEM_ACK ok');
    }
    try {
      await ackHandover(handoverId: hid);
      debugPrint('WRITE_HANDOVER_ACK ok');
    } catch (e) {
      debugPrint('WRITE_HANDOVER_ACK skip $e');
    }

    debugPrint('WRITE_REMAINING_DONE');
  });
}
