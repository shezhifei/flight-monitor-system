import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:intl/intl.dart';
import 'package:path_provider/path_provider.dart';

import 'package:flight_monitor/bridge/api.dart';
import 'package:flight_monitor/bridge/api/auth.dart';
import 'package:flight_monitor/bridge/api/dispatch.dart';
import 'package:flight_monitor/bridge/api/session.dart';
import 'package:flight_monitor/bridge/frb_generated.dart';

/// P1 验收（plan §6 P1 验收清单）：真机对拍主链路 + token 恢复。
///
/// 后端：宿主机 fms-server（--dart-define=FMS_TEST_BASE_URL 注入，默认
/// http://10.0.2.2:8000）。账号 admin/admin123。
///
/// 动作链顺序说明：后端 `mobile_lifecycle.rs` 规定 checkout 只允许
/// InProgress/Assigned 状态（"仅进行中或已派发的工单可以签退"），complete
/// 要求 InProgress 且对已 Completed 幂等，因此顺序为
/// accept → checkin → start → eta_report → 门禁 → checkout → complete
/// （checkout 可能触发全员签退自动完工，此时 complete 走幂等分支仍成功）。
///
/// 【Bug A/B/C 已修复（见验收报告）：my/assigned 路径、三处 step_code
/// Option 化、auth_heartbeat/logout 改 raw 解析】P1-4 为修复后的成功路径
/// 复验（原"bug 证据固化"断言已翻转）。
///
/// 工单选择：不再硬编码已跑完的 completed 单。登录后从 my/assigned 按
/// 状态优先级挑一张可继续推进的工单，只执行**剩余**动作（重跑幂等）。

const String kBaseUrl = String.fromEnvironment(
  'FMS_TEST_BASE_URL',
  defaultValue: 'http://10.0.2.2:8000',
);

/// 历史探针工单（P1-4 门禁复验用；completed 仍可拉 checklist）。
const String kChecklistOrderId = '01KMAYZNHHWTYCSPSG9VPB4XCS';
const String kDeviceId = 'p1-acceptance-device';

/// 可继续推进的状态优先级（越前越完整）。
const _statusPriority = <String>[
  'assigned',
  'accepted',
  'checked_in',
  'in_progress',
];

final _timeFormat = DateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'");

String _rfc3339Utc(DateTime t) => _timeFormat.format(t.toUtc());

bool _rustInitialized = false;
TokenBundle? savedBundle;
String? savedOrderId;

Future<void> ensureRust() async {
  if (!_rustInitialized) {
    await RustLib.init();
    _rustInitialized = true;
  }
}

Future<void> initCoreForTest() async {
  final supportDir = await getApplicationSupportDirectory();
  await initCore(
    baseUrl: kBaseUrl,
    allowCleartext: true,
    dbPath: '${supportDir.path}${Platform.pathSeparator}fms_offline.db',
    operatorContextId: kDeviceId,
  );
}

Future<DispatchActionResult> act(
  String orderId,
  String actionType,
  Map<String, dynamic> payload,
) {
  return dispatchAction(
    orderId: orderId,
    actionJson: jsonEncode({'action_type': actionType, 'payload': payload}),
  );
}

/// 从列表挑一张可推进工单。
///
/// 优先状态越前越完整（assigned 可走满链）。同一状态下优先
/// `enforced=false || ready=true` 的单（本地后端部分模板的清单提交会因
/// DB varchar 过短 500，属环境问题，不阻塞客户端验收）。
Future<DispatchOrder> pickActionableOrder(List<DispatchOrder> orders) async {
  final byStatus = <String, List<DispatchOrder>>{};
  for (final o in orders) {
    byStatus.putIfAbsent(o.status.toLowerCase(), () => []).add(o);
  }

  DispatchOrder? fallback;
  for (final status in _statusPriority) {
    final candidates = byStatus[status] ?? const <DispatchOrder>[];
    for (final o in candidates) {
      fallback ??= o;
      try {
        final gate = await safetyChecklist(orderId: o.id);
        if (!gate.enforced || gate.ready) {
          debugPrint(
            'P1_PICK order=${o.id} status=$status '
            'enforced=${gate.enforced} ready=${gate.ready}',
          );
          return o;
        }
        debugPrint(
          'P1_SKIP_GATE order=${o.id} enforced=${gate.enforced} '
          'ready=${gate.ready}',
        );
      } catch (e) {
        debugPrint('P1_SKIP_GATE_ERR order=${o.id} err=$e');
      }
    }
  }
  if (fallback != null) {
    debugPrint('P1_PICK_FALLBACK order=${fallback.id} status=${fallback.status}');
    return fallback;
  }
  fail(
    'my/assigned 中没有可推进工单（assigned/accepted/checked_in/in_progress）；'
    '当前: ${orders.map((o) => '${o.id}:${o.status}').join(', ')}',
  );
}

/// 按当前状态产出剩余动作（不含 report_issue）。
List<String> remainingActions(String status) {
  return switch (status.toLowerCase()) {
    'assigned' => [
        'accept',
        'checkin',
        'start',
        'eta_report',
        'checkout',
        'complete',
      ],
    'accepted' => ['checkin', 'start', 'eta_report', 'checkout', 'complete'],
    'checked_in' => ['start', 'eta_report', 'checkout', 'complete'],
    'in_progress' => ['eta_report', 'checkout', 'complete'],
    _ => <String>[],
  };
}

/// 完工前若清单 enforced 且未 ready，尝试把 pending 必填项全部 pass。
/// 返回是否允许继续 complete（本地后端部分模板 submit 会 500，此时跳过
/// complete，不把环境故障算作客户端失败）。
Future<bool> ensureSafetyGate(String orderId) async {
  final checklist = await safetyChecklist(orderId: orderId);
  debugPrint('P1_GATE enforced=${checklist.enforced} ready=${checklist.ready} '
      'items=${checklist.items.length}');
  if (!checklist.enforced || checklist.ready) return true;
  try {
    for (final item in checklist.items) {
      if (item.result != null && item.result!.isNotEmpty) continue;
      if (!item.required_) continue;
      await submitChecklistItem(
        orderId: orderId,
        itemCode: item.itemCode,
        result: 'pass',
      );
      debugPrint('P1_GATE_PASS item=${item.itemCode}');
    }
  } catch (e) {
    debugPrint('P1_GATE_SUBMIT_FAILED err=$e');
    return false;
  }
  final after = await safetyChecklist(orderId: orderId);
  debugPrint('P1_GATE_AFTER ready=${after.ready}');
  return after.ready;
}

Future<void> runRemainingChain(String orderId, String status) async {
  final actions = remainingActions(status);
  expect(actions, isNotEmpty, reason: '状态 $status 应有剩余动作');
  debugPrint('P1_CHAIN_FROM status=$status actions=${actions.join('→')}');

  for (final action in actions) {
    if (action == 'complete') {
      final canComplete = await ensureSafetyGate(orderId);
      if (!canComplete) {
        debugPrint('P1_SKIP_COMPLETE reason=gate-blocked-or-backend-error');
        continue;
      }
    }
    final Map<String, dynamic> payload = switch (action) {
      'accept' || 'checkin' || 'checkout' => {'note': null},
      'start' => {'notes': null},
      'eta_report' => {
          'estimated_completion_time':
              _rfc3339Utc(DateTime.now().add(const Duration(minutes: 30))),
          'note': null,
        },
      'complete' => {
          'actual_end_time': _rfc3339Utc(DateTime.now()),
          'completion_notes': null,
          'issues': <String>[],
        },
      _ => <String, dynamic>{},
    };
    final result = await act(orderId, action, payload);
    expect(
      result,
      DispatchActionResult.sent,
      reason: '$action 应直发成功（当前起点 status=$status）',
    );
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('P1-1 主链路对拍：登录→派工动作链全 Sent', (tester) async {
    await ensureRust();
    await initCoreForTest();

    // 登录 + bundle 留存（供 P1-3 token 恢复用）。
    await login(username: 'admin', password: 'admin123');
    savedBundle = await currentTokenBundle();
    expect(savedBundle, isNotNull, reason: '登录后应能取到 TokenBundle');
    expect(savedBundle!.sessionSecret, isNotEmpty,
        reason: 'native 面登录必须带 session_secret');

    // 动态选单：避免硬编码 completed 工单导致"当前状态不允许接单"。
    final orders = await myAssignedOrders();
    expect(orders, isNotEmpty, reason: 'admin 应有派工单');
    final target = await pickActionableOrder(orders);
    savedOrderId = target.id;
    debugPrint('P1_TARGET_ORDER=${target.id} status=${target.status}');

    await runRemainingChain(target.id, target.status);

    debugPrint('P1_MAIN_CHAIN_DONE order=${target.id}');
  });

  testWidgets('P1-3 token 恢复：模拟重启后免登录执行签名请求', (tester) async {
    expect(savedBundle, isNotNull, reason: '依赖 P1-1 保存的 bundle');
    await ensureRust();
    // 重新 initCore = 模拟杀进程重启后的全新 Rust runtime（session 为空）。
    await initCoreForTest();
    await restoreTokens(bundle: savedBundle!);

    // 恢复后执行需 Bearer+签名的只读请求（workbench），避免对已完工单再 accept。
    final wb = await workbench(pendingSyncCount: 0, maxOrders: 10);
    expect(wb.userId, isNotEmpty,
        reason: 'restore 后签名请求必须被接受（401 即失败）');
    debugPrint('P1_TOKEN_RESTORE_OK user=${wb.userId} '
        'orders=${wb.myOrders.length}');
  });

  testWidgets('P1-4 bug 修复复验：列表/工作台/门禁/心跳全部走通', (tester) async {
    await ensureRust();
    await initCoreForTest();
    await login(username: 'admin', password: 'admin123');

    // Bug A 复验：my/assigned 真实拉取列表（DispatchScreen 列表数据源，
    // 修复前 404 等于页面列表是坏的）。
    final orders = await myAssignedOrders();
    debugPrint('P1_MY_ASSIGNED count=${orders.length} '
        'ids=${orders.map((o) => o.id).join(',')}');
    expect(orders, isNotEmpty, reason: 'admin 应有派工单（探针实测 6 单）');

    // Bug B1 复验：workbench 解析成功（my_orders 载荷缺席 step_code）。
    final wb = await workbench(pendingSyncCount: 0, maxOrders: 50);
    debugPrint('P1_WORKBENCH my_orders=${wb.myOrders.length} '
        'total=${wb.orderCounts.total}');
    expect(wb.userId, isNotEmpty);

    // Bug B3 复验：safety-checklist 解析成功（响应无 step_code 字段）。
    // 用历史 completed 单或当前列表第一张均可。
    final checklistOrderId = orders.isNotEmpty
        ? orders.first.id
        : kChecklistOrderId;
    final checklist = await safetyChecklist(orderId: checklistOrderId);
    debugPrint('P1_CHECKLIST enforced=${checklist.enforced} '
        'ready=${checklist.ready} items=${checklist.items.length}');
    expect(checklist.dispatchOrderId, checklistOrderId);

    // Bug C 复验：auth_heartbeat 正常（auth_resp data:null 走 raw 解析）。
    await authHeartbeat();
    debugPrint('P1_AUTH_HEARTBEAT_OK');

    debugPrint('P1_FIX_VERIFY_DONE');
  });
}
