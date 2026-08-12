import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/constants.dart';
import '../bridge/api.dart';
import '../bridge/api/auth.dart';
import '../bridge/api/dispatch.dart';
import 'session_provider.dart';
import 'settings_provider.dart';

/// 工作台数据（plan §5 WorkbenchScreen）。
final workbenchProvider =
    AsyncNotifierProvider<WorkbenchNotifier, Workbench>(WorkbenchNotifier.new);

class WorkbenchNotifier extends AsyncNotifier<Workbench> {
  @override
  Future<Workbench> build() async {
    final pendingSync = ref.watch(pendingSyncCountProvider);
    return workbench(
      pendingSyncCount: pendingSync,
      maxOrders: AppConstants.workbenchMaxOrders,
    );
  }

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(() => build());
  }
}

/// SSE 实时事件流（登录后才连接；plan P0-5 已联调 /api/v2/sse/stream）。
final sseUpdatesProvider = StreamProvider<SseUpdate>((ref) {
  if (!ref.watch(loggedInProvider)) return const Stream.empty();
  return notificationsStream();
});

/// SSE 连接状态标签（指示灯用）：连接中/已连接/已断开。
enum SseIndicator { connecting, connected, disconnected }

final sseIndicatorProvider = Provider<SseIndicator>((ref) {
  final updates = ref.watch(sseUpdatesProvider);
  final latest = updates.value;
  if (latest is SseUpdate_State) {
    return switch (latest.field0) {
      SseConnectionState_Connected() => SseIndicator.connected,
      SseConnectionState_Disconnected() => SseIndicator.disconnected,
      SseConnectionState_Connecting() => SseIndicator.connecting,
    };
  }
  return SseIndicator.connecting;
});

/// 60s 心跳循环（plan §5 constants）：auth 心跳 + 设备心跳 + 离线补传。
/// 各项独立容错，网络异常不打断 UI。
Future<void> runHeartbeatCycle(WidgetRef ref) async {
  try {
    await authHeartbeat();
  } catch (_) {}

  try {
    final results = await Connectivity().checkConnectivity();
    final networkStatus = results.map((r) => r.name).join(',');
    await deviceHeartbeat(
      meta: DeviceHeartbeatMeta(networkStatus: networkStatus),
    );
  } catch (_) {}

  try {
    final summary = await syncOfflineActions();
    ref.read(pendingSyncCountProvider.notifier).set(summary.remaining);
  } catch (_) {}

  // 工作台计数（含服务端 pending_sync_action_count）可能已变化。
  try {
    await ref.read(workbenchProvider.notifier).refresh();
  } catch (_) {}
}
