import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../app/l10n.dart';
import '../../providers/notification_provider.dart';

/// 回执组详情。
class ReceiptGroupScreen extends ConsumerWidget {
  const ReceiptGroupScreen({super.key, required this.receiptGroupId});

  final String receiptGroupId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final async = ref.watch(receiptGroupProvider(receiptGroupId));

    return Scaffold(
      appBar: AppBar(title: const Text(S.notificationsReceiptGroup)),
      body: async.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('${S.errorPrefix}$e')),
        data: (group) {
          final s = group.summary;
          return ListView(
            padding: const EdgeInsets.all(16),
            children: [
              Text(group.title ?? group.receiptGroupId,
                  style: Theme.of(context).textTheme.titleMedium),
              if (group.flightId != null)
                Text('${S.notificationsFlight}: ${group.flightId}'),
              const SizedBox(height: 12),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  Chip(label: Text('${S.receiptTotal}: ${s.totalCount}')),
                  Chip(label: Text('${S.receiptPending}: ${s.pendingCount}')),
                  Chip(
                      label:
                          Text('${S.receiptAcked}: ${s.acknowledgedCount}')),
                  Chip(
                      label:
                          Text('${S.receiptRejected}: ${s.rejectedCount}')),
                ],
              ),
              const SizedBox(height: 16),
              Text(S.receiptItems,
                  style: Theme.of(context).textTheme.titleSmall),
              const SizedBox(height: 8),
              for (final r in group.items)
                Card(
                  child: ListTile(
                    title: Text(r.title ?? r.notificationId),
                    subtitle: Text(
                      '${r.userId} · ${r.ackStatus}'
                      '${r.ackNote != null ? ' · ${r.ackNote}' : ''}',
                    ),
                    trailing: Text(r.readStatus),
                  ),
                ),
            ],
          );
        },
      ),
    );
  }
}
