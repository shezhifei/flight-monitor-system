import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api/dispatch.dart';
import 'settings_provider.dart';

/// 派工动作（对照旧 App DispatchActionsActivity 的状态机按钮）。
enum DispatchActionId { accept, checkIn, checkOut, start, complete, etaReport }

/// 状态 → 可用主流程动作（旧 App updateActionButtonsForStatus）：
/// assigned→接单，accepted→签到，checked_in→开始作业，
/// in_progress→上报预计/完工/签退；安全检查清单与问题上报另行常驻。
List<DispatchActionId> dispatchActionsForStatus(String status) {
  return switch (status.toLowerCase()) {
    'assigned' => [DispatchActionId.accept],
    'accepted' => [DispatchActionId.checkIn],
    'checked_in' => [DispatchActionId.start],
    'in_progress' => [
        DispatchActionId.etaReport,
        DispatchActionId.complete,
        DispatchActionId.checkOut,
      ],
    _ => const [],
  };
}

/// 安全检查清单入口可见的状态（旧 App：in_progress）。
bool safetyChecklistVisibleForStatus(String status) =>
    status.toLowerCase() == 'in_progress';

/// 问题上报入口可见的状态（终态之前都允许）。
bool reportIssueVisibleForStatus(String status) =>
    !const ['completed', 'cancelled'].contains(status.toLowerCase());

/// action_type 字符串（Rust dispatch_action 契约）。
String dispatchActionType(DispatchActionId action) => switch (action) {
      DispatchActionId.accept => 'accept',
      DispatchActionId.checkIn => 'checkin',
      DispatchActionId.checkOut => 'checkout',
      DispatchActionId.start => 'start',
      DispatchActionId.complete => 'complete',
      DispatchActionId.etaReport => 'eta_report',
    };

/// 动作执行结果（供 Snackbar 反馈）。
enum DispatchActionFeedback { sent, queued }

/// 派工工单列表（plan §5 DispatchScreen：my/assigned）。
final dispatchOrdersProvider = AsyncNotifierProvider<DispatchOrdersNotifier,
    List<DispatchOrder>>(DispatchOrdersNotifier.new);

class DispatchOrdersNotifier extends AsyncNotifier<List<DispatchOrder>> {
  @override
  Future<List<DispatchOrder>> build() => myAssignedOrders();

  Future<void> refresh() async {
    state = const AsyncLoading();
    state = await AsyncValue.guard(myAssignedOrders);
  }

  /// 执行一个派工动作（Rust 侧决定直发或入离线队列）。
  /// 成功后刷新列表；入队则更新待同步计数。
  Future<DispatchActionFeedback> execute(
    String orderId,
    DispatchActionId action,
    Map<String, dynamic> payload,
  ) async {
    final result = await dispatchAction(
      orderId: orderId,
      actionJson: jsonEncode({
        'action_type': dispatchActionType(action),
        'payload': payload,
      }),
    );
    switch (result) {
      case DispatchActionResult.sent:
        await refresh();
        return DispatchActionFeedback.sent;
      case DispatchActionResult.queued:
        final current = ref.read(pendingSyncCountProvider);
        ref.read(pendingSyncCountProvider.notifier).set(current + 1);
        return DispatchActionFeedback.queued;
    }
  }

  /// 问题上报（report_issue 走同一 dispatch_action 通道）。
  Future<DispatchActionFeedback> reportIssue(
    String orderId, {
    required String title,
    String? description,
    List<String> attachments = const [],
  }) async {
    final result = await dispatchAction(
      orderId: orderId,
      actionJson: jsonEncode({
        'action_type': 'report_issue',
        'payload': {
          'title': title,
          'description': description,
          'severity': 'medium',
          'issue_type': 'dispatch_issue',
          'attachments': attachments,
        },
      }),
    );
    if (result == DispatchActionResult.queued) {
      final current = ref.read(pendingSyncCountProvider);
      ref.read(pendingSyncCountProvider.notifier).set(current + 1);
    }
    return result == DispatchActionResult.sent
        ? DispatchActionFeedback.sent
        : DispatchActionFeedback.queued;
  }
}

/// 安全检查清单（按工单）。
final safetyChecklistProvider =
    FutureProvider.family<SafetyChecklist, String>(
  (ref, orderId) => safetyChecklist(orderId: orderId),
);
