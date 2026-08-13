import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/l10n.dart';
import '../../bridge/api/notification.dart' as notif_api;
import '../../providers/notification_provider.dart';
import '../../providers/sse_demux.dart';
import '../../shared/widgets/snackbar.dart';

/// 通知列表。
class NotificationsScreen extends ConsumerWidget {
  const NotificationsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    ref.watch(sseDemuxProvider);
    final list = ref.watch(notificationsProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text(S.notificationsTitle),
        actions: [
          TextButton(
            onPressed: () async {
              final ok = await showDialog<bool>(
                context: context,
                builder: (ctx) => AlertDialog(
                  title: const Text(S.notificationsReadAll),
                  content: const Text(S.notificationsReadAllConfirm),
                  actions: [
                    TextButton(
                      onPressed: () => Navigator.pop(ctx, false),
                      child: const Text(S.cancel),
                    ),
                    FilledButton(
                      onPressed: () => Navigator.pop(ctx, true),
                      child: const Text(S.confirm),
                    ),
                  ],
                ),
              );
              if (ok != true) return;
              try {
                await ref.read(notificationsProvider.notifier).markAllRead();
                await ref.read(unreadCountProvider.notifier).refresh();
                if (context.mounted) {
                  showAppSnackBar(context, S.notificationsReadAllDone);
                }
              } catch (e) {
                if (context.mounted) showErrorSnackBar(context, e);
              }
            },
            child: const Text(S.notificationsReadAll),
          ),
        ],
      ),
      body: RefreshIndicator(
        onRefresh: () async {
          await ref.read(notificationsProvider.notifier).refresh();
          await ref.read(unreadCountProvider.notifier).refresh();
        },
        child: list.when(
          loading: () => const Center(child: CircularProgressIndicator()),
          error: (e, _) => ListView(
            children: [
              const SizedBox(height: 120),
              Center(child: Text('${S.errorPrefix}$e')),
              Center(
                child: FilledButton(
                  onPressed: () =>
                      ref.read(notificationsProvider.notifier).refresh(),
                  child: const Text(S.retry),
                ),
              ),
            ],
          ),
          data: (items) {
            if (items.isEmpty) {
              return ListView(
                children: const [
                  SizedBox(height: 120),
                  Center(child: Text(S.notificationsEmpty)),
                ],
              );
            }
            return ListView.separated(
              itemCount: items.length,
              separatorBuilder: (_, _) => const Divider(height: 1),
              itemBuilder: (context, i) => _NotifTile(item: items[i]),
            );
          },
        ),
      ),
    );
  }
}

class _NotifTile extends StatelessWidget {
  const _NotifTile({required this.item});
  final notif_api.Notification item;

  @override
  Widget build(BuildContext context) {
    final unread = !item.isRead && item.readStatus != 'read';
    return ListTile(
      leading: Icon(
        unread ? Icons.mark_email_unread : Icons.mark_email_read_outlined,
        color: unread ? Theme.of(context).colorScheme.primary : null,
      ),
      title: Text(
        item.title,
        style: TextStyle(
          fontWeight: unread ? FontWeight.w600 : FontWeight.normal,
        ),
      ),
      subtitle: Text(
        item.body,
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Text(item.severity,
              style: Theme.of(context).textTheme.labelSmall),
          if (item.receiptRequired)
            Text(S.notificationsReceiptRequired,
                style: Theme.of(context).textTheme.labelSmall),
        ],
      ),
      onTap: () => context.push('/notifications/${item.notificationId}'),
    );
  }
}
