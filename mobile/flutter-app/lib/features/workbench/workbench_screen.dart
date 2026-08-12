import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/constants.dart';
import '../../app/l10n.dart';
import '../../bridge/api/dispatch.dart';
import '../../providers/notification_provider.dart';
import '../../providers/sse_demux.dart';
import '../../providers/workbench_provider.dart';
import '../dispatch/status_label.dart';

/// 工作台（plan §5 WorkbenchScreen）：workbench 概览 + 60s 心跳循环 +
/// SSE 连接指示灯 + P2 入口（消息/通知/交接）。
class WorkbenchScreen extends ConsumerStatefulWidget {
  const WorkbenchScreen({super.key});

  @override
  ConsumerState<WorkbenchScreen> createState() => _WorkbenchScreenState();
}

class _WorkbenchScreenState extends ConsumerState<WorkbenchScreen> {
  Timer? _heartbeat;

  @override
  void initState() {
    super.initState();
    _heartbeat =
        Timer.periodic(AppConstants.heartbeatInterval, (_) => _tick());
  }

  void _tick() {
    if (!mounted) return;
    runHeartbeatCycle(ref).catchError((_) {});
  }

  @override
  void dispose() {
    _heartbeat?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final data = ref.watch(workbenchProvider);
    final indicator = ref.watch(sseIndicatorProvider);
    // SSE 流 + demux（聊天/通知增量）持续存活。
    ref.watch(sseUpdatesProvider);
    ref.watch(sseDemuxProvider);
    final unread = ref.watch(unreadCountProvider).asData?.value;

    return Scaffold(
      appBar: AppBar(
        title: const Text(S.navWorkbench),
        actions: [
          Padding(
            padding: const EdgeInsets.only(right: 16),
            child: _SseIndicator(indicator: indicator),
          ),
        ],
      ),
      body: RefreshIndicator(
        onRefresh: () => ref.read(workbenchProvider.notifier).refresh(),
        child: data.when(
          loading: () => const Center(child: CircularProgressIndicator()),
          error: (e, _) => ListView(
            children: [
              const SizedBox(height: 120),
              Center(child: Text('${S.errorPrefix}$e')),
              const SizedBox(height: 16),
              Center(
                child: FilledButton(
                  onPressed: () =>
                      ref.read(workbenchProvider.notifier).refresh(),
                  child: const Text(S.retry),
                ),
              ),
            ],
          ),
          data: (wb) => _WorkbenchBody(
            workbench: wb,
            unreadOverride: unread,
          ),
        ),
      ),
    );
  }
}

class _SseIndicator extends StatelessWidget {
  const _SseIndicator({required this.indicator});

  final SseIndicator indicator;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final (color, label) = switch (indicator) {
      SseIndicator.connected => (scheme.primary, S.sseConnected),
      SseIndicator.connecting => (scheme.tertiary, S.sseConnecting),
      SseIndicator.disconnected => (scheme.error, S.sseDisconnected),
    };
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.circle, size: 10, color: color),
        const SizedBox(width: 6),
        Text(label, style: Theme.of(context).textTheme.labelSmall),
      ],
    );
  }
}

class _WorkbenchBody extends StatelessWidget {
  const _WorkbenchBody({required this.workbench, this.unreadOverride});

  final Workbench workbench;
  final int? unreadOverride;

  @override
  Widget build(BuildContext context) {
    final counts = workbench.orderCounts;
    final notifUnread =
        unreadOverride ?? workbench.notificationUnreadCount.toInt();
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            _CountCard(
                label: S.statusPending,
                count: counts.pending.toInt(),
                onTap: () => context.go('/dispatch')),
            _CountCard(
                label: S.statusAssigned,
                count: counts.assigned.toInt(),
                onTap: () => context.go('/dispatch')),
            _CountCard(
                label: S.statusInProgress,
                count: counts.inProgress.toInt(),
                onTap: () => context.go('/dispatch')),
            _CountCard(
                label: S.statusCompleted, count: counts.completed.toInt()),
          ],
        ),
        const SizedBox(height: 16),
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            _CountCard(
                label: S.workbenchUnreadNotifications,
                count: notifUnread,
                onTap: () => context.go('/notifications')),
            _CountCard(
                label: S.workbenchUnreadChat,
                count: workbench.chatUnreadTotal.toInt(),
                onTap: () => context.go('/chat')),
            _CountCard(
                label: S.workbenchPendingHandover,
                count: workbench.pendingShiftHandoverCount.toInt(),
                onTap: () => context.push('/handover')),
            _CountCard(
                label: S.workbenchPendingSync,
                count: workbench.pendingSyncActionCount.toInt()),
          ],
        ),
        const SizedBox(height: 24),
        Text(S.workbenchMyOrders,
            style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        if (workbench.myOrders.isEmpty)
          const Card(
            child: Padding(
              padding: EdgeInsets.all(16),
              child: Text(S.dispatchEmpty),
            ),
          )
        else
          for (final order in workbench.myOrders)
            Card(
              child: ListTile(
                title: Text('${order.flightId} · ${order.stepCode ?? '-'}'),
                subtitle: Text([
                  if (order.terminal != null) order.terminal!,
                  if (order.standId != null) order.standId!,
                  if (order.gate != null) order.gate!,
                ].join(' ')),
                trailing: StatusChip(status: order.status),
                onTap: () => context.go('/dispatch'),
              ),
            ),
        const SizedBox(height: 24),
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            OutlinedButton.icon(
              onPressed: () => context.go('/chat'),
              icon: const Icon(Icons.chat_bubble_outline),
              label: const Text(S.navChat),
            ),
            OutlinedButton.icon(
              onPressed: () => context.go('/notifications'),
              icon: const Icon(Icons.notifications_outlined),
              label: const Text(S.navNotifications),
            ),
            OutlinedButton.icon(
              onPressed: () => context.push('/handover'),
              icon: const Icon(Icons.swap_horiz),
              label: const Text(S.navHandover),
            ),
            OutlinedButton.icon(
              onPressed: () => context.push('/business-cases'),
              icon: const Icon(Icons.folder_special_outlined),
              label: const Text(S.navBusinessCase),
            ),
            OutlinedButton.icon(
              onPressed: () => context.push('/operations'),
              icon: const Icon(Icons.radar_outlined),
              label: const Text(S.navOperations),
            ),
          ],
        ),
      ],
    );
  }
}

class _CountCard extends StatelessWidget {
  const _CountCard({
    required this.label,
    required this.count,
    this.onTap,
  });

  final String label;
  final int count;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 160,
      child: Card(
        child: InkWell(
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('$count',
                    style: Theme.of(context).textTheme.headlineMedium),
                const SizedBox(height: 4),
                Text(label, style: Theme.of(context).textTheme.labelMedium),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
