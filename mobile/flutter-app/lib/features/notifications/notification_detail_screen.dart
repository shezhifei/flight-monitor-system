import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/l10n.dart';
import '../../bridge/api/notification.dart' as notif_api;
import '../../providers/notification_provider.dart';
import '../../shared/widgets/snackbar.dart';

/// 通知详情 + ack/reject（plan §5 NotificationDetailScreen）。
class NotificationDetailScreen extends ConsumerWidget {
  const NotificationDetailScreen({super.key, required this.notificationId});

  final String notificationId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final list = ref.watch(notificationsProvider);
    final item = list.asData?.value
        .where((n) => n.notificationId == notificationId)
        .firstOrNull;

    if (list.isLoading && item == null) {
      return const Scaffold(body: Center(child: CircularProgressIndicator()));
    }
    if (item == null) {
      return Scaffold(
        appBar: AppBar(title: const Text(S.notificationDetailTitle)),
        body: Center(
          child: FilledButton(
            onPressed: () =>
                ref.read(notificationsProvider.notifier).refresh(),
            child: const Text(S.retry),
          ),
        ),
      );
    }

    return Scaffold(
      appBar: AppBar(title: Text(item.title)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(item.body, style: Theme.of(context).textTheme.bodyLarge),
          const SizedBox(height: 12),
          _Meta(label: S.notificationsSeverity, value: item.severity),
          _Meta(label: S.notificationsCategory, value: item.category),
          _Meta(label: S.notificationsOrigin, value: item.originLabel),
          _Meta(label: S.notificationsCreatedAt, value: item.createdAt),
          _Meta(label: S.notificationsAckStatus, value: item.ackStatus),
          if (item.receiptGroupId != null) ...[
            const SizedBox(height: 8),
            OutlinedButton.icon(
              onPressed: () => context.push(
                '/notifications/receipt-groups/${item.receiptGroupId}',
              ),
              icon: const Icon(Icons.groups_outlined),
              label: const Text(S.notificationsReceiptGroup),
            ),
          ],
          const SizedBox(height: 24),
          if (!item.isRead)
            FilledButton(
              onPressed: () async {
                try {
                  await ref
                      .read(notificationsProvider.notifier)
                      .markRead(item.notificationId);
                  await ref.read(unreadCountProvider.notifier).refresh();
                  if (context.mounted) {
                    showAppSnackBar(context, S.notificationsMarkedRead);
                  }
                } catch (e) {
                  if (context.mounted) showErrorSnackBar(context, e);
                }
              },
              child: const Text(S.notificationsMarkRead),
            ),
          if (item.receiptRequired && item.ackStatus == 'pending') ...[
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: FilledButton.tonal(
                    onPressed: () => _ack(context, ref, item, 'ack'),
                    child: const Text(S.notificationsAck),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: OutlinedButton(
                    onPressed: () => _ack(context, ref, item, 'reject'),
                    child: const Text(S.notificationsReject),
                  ),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _ack(
    BuildContext context,
    WidgetRef ref,
    notif_api.Notification item,
    String action,
  ) async {
    final noteController = TextEditingController();
    final needNote = action == 'reject';
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(action == 'ack' ? S.notificationsAck : S.notificationsReject),
        content: TextField(
          controller: noteController,
          decoration: InputDecoration(
            hintText: needNote ? S.notificationsNoteRequired : S.notificationsNoteOptional,
          ),
          maxLines: 3,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text(S.cancel),
          ),
          FilledButton(
            onPressed: () {
              if (needNote && noteController.text.trim().isEmpty) return;
              Navigator.pop(ctx, true);
            },
            child: const Text(S.confirm),
          ),
        ],
      ),
    );
    if (ok != true) return;
    final note = noteController.text.trim();
    if (needNote && note.isEmpty) {
      if (context.mounted) {
        showAppSnackBar(context, S.notificationsNoteRequired, isError: true);
      }
      return;
    }
    try {
      await ref.read(notificationsProvider.notifier).ack(
            item.notificationId,
            action,
            note: note.isEmpty ? null : note,
          );
      await ref.read(unreadCountProvider.notifier).refresh();
      if (context.mounted) showAppSnackBar(context, S.actionSent);
    } catch (e) {
      if (context.mounted) showErrorSnackBar(context, e);
    }
  }
}

class _Meta extends StatelessWidget {
  const _Meta({required this.label, required this.value});
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 96,
            child: Text(label,
                style: Theme.of(context).textTheme.labelMedium),
          ),
          Expanded(child: Text(value)),
        ],
      ),
    );
  }
}
