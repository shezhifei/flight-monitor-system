import 'dart:convert';
import 'dart:io';

import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path_provider/path_provider.dart';

import 'package:flight_monitor/bridge/api.dart';
import 'package:flight_monitor/bridge/api/auth.dart';
import 'package:flight_monitor/bridge/api/dispatch.dart';
import 'package:flight_monitor/bridge/frb_generated.dart';

/// 离线补传（applied 分支），由宿主机脚本切换飞行模式。
///
/// 编排（logcat marker 驱动，flutter 日志 tag 为 `flutter`）：
/// 1. 登录后打印 `OFFLINE_READY_FOR_AIRPLANE`，然后轮询 connectivity；
/// 2. 宿主机看到 marker 后 `adb shell cmd connectivity airplane-mode enable`；
/// 3. 测试发现断网 → dispatch_action（checkin）→ 断言 Queued →
///    打印 `OFFLINE_QUEUED`；宿主机 `airplane-mode disable`；
/// 4. 网络恢复 → 等 3s → sync_offline_actions → 断言 applied ≥ 1，
///    打印 `OFFLINE_SYNC_RESULT`。
///
/// 目标工单动态从 my/assigned 挑一张非终态单；checkin 在后端仅禁止
/// Completed/Cancelled，因此 accepted/checked_in/in_progress 均可入队
/// （幂等，且断网时只验证入队/补传，不依赖状态推进）。

const String kBaseUrl = String.fromEnvironment(
  'FMS_TEST_BASE_URL',
  defaultValue: 'http://10.0.2.2:8000',
);
const String kDeviceId = 'offline-sync-device';

bool _rustInitialized = false;

Future<void> ensureRust() async {
  if (!_rustInitialized) {
    await RustLib.init();
    _rustInitialized = true;
  }
}

Future<bool> _isOffline() async {
  final results = await Connectivity().checkConnectivity();
  return results.isEmpty || results.every((r) => r == ConnectivityResult.none);
}

Future<void> _waitConnectivity(
  bool offline, {
  Duration timeout = const Duration(minutes: 8),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    if (await _isOffline() == offline) return;
    await Future.delayed(const Duration(seconds: 2));
  }
  fail('等待网络${offline ? "断开" : "恢复"}超时（${timeout.inMinutes} 分钟）——'
      '宿主机未按 marker 切换飞行模式？');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    '离线补传：断网入队→恢复→sync applied',
    (tester) async {
      await ensureRust();
      final supportDir = await getApplicationSupportDirectory();
      await initCore(
        baseUrl: kBaseUrl,
        allowCleartext: true,
        dbPath: '${supportDir.path}${Platform.pathSeparator}fms_offline.db',
        operatorContextId: kDeviceId,
      );
      await login(username: 'admin', password: 'admin123');

      // 动态选非终态工单（避免硬编码已完工 id）。
      final orders = await myAssignedOrders();
      final candidates = orders
          .where((o) {
            final s = o.status.toLowerCase();
            return s != 'completed' && s != 'cancelled';
          })
          .toList();
      expect(candidates, isNotEmpty, reason: '需要至少一张非终态工单做离线入队');
      final target = candidates.first;
      debugPrint('OFFLINE_ORDER=${target.id} status=${target.status}');

      // 等宿主机开飞行模式。
      debugPrint('OFFLINE_READY_FOR_AIRPLANE');
      await _waitConnectivity(true);

      // 断网执行动作 → 必须入离线队列（仅网络类错误入队）。
      final result = await dispatchAction(
        orderId: target.id,
        actionJson: jsonEncode({
          'action_type': 'checkin',
          'payload': {'note': 'offline-acceptance'},
        }),
      );
      expect(result, DispatchActionResult.queued,
          reason: '断网时 dispatch_action 必须入队');
      debugPrint('OFFLINE_QUEUED');

      // 等宿主机恢复网络，稳定 3s 后补传。
      await _waitConnectivity(false);
      await Future.delayed(const Duration(seconds: 3));
      final summary = await syncOfflineActions();
      debugPrint('OFFLINE_SYNC_RESULT total=${summary.total} '
          'applied=${summary.applied} duplicates=${summary.duplicates} '
          'failed=${summary.failed} remaining=${summary.remaining}');
      expect(summary.applied >= 1, isTrue,
          reason: '补传必须至少 applied 一条；failed=${summary.failed}');
      expect(summary.remaining, 0, reason: 'applied 后队列应清空');
    },
    timeout: const Timeout(Duration(minutes: 20)),
  );
}
