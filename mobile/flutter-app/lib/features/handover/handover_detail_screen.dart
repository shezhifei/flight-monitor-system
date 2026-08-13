import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../app/l10n.dart';
import '../../providers/handover_provider.dart';
import '../../shared/widgets/snackbar.dart';

/// 交接班详情：条目签收 + 整单签收。
class HandoverDetailScreen extends ConsumerWidget {
  const HandoverDetailScreen({super.key, required this.handoverId});

  final String handoverId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final async = ref.watch(handoverDetailProvider(handoverId));

    return Scaffold(
      appBar: AppBar(title: const Text(S.handoverDetailTitle)),
      body: async.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('${S.errorPrefix}$e')),
        data: (h) {
          final pendingItems =
              h.items.where((i) => !i.acknowledged).length;
          return ListView(
            padding: const EdgeInsets.all(16),
            children: [
              Text('${h.shiftDate} · ${h.shiftCode}',
                  style: Theme.of(context).textTheme.titleMedium),
              Text('${S.handoverStatus}: ${h.status}'),
              Text('${S.handoverRisk}: ${h.riskLevel}'),
              if (h.summary != null) ...[
                const SizedBox(height: 8),
                Text(h.summary!),
              ],
              const SizedBox(height: 16),
              Text(S.handoverItems,
                  style: Theme.of(context).textTheme.titleSmall),
              const SizedBox(height: 8),
              for (final item in h.items)
                Card(
                  child: ListTile(
                    title: Text(item.title),
                    subtitle: Text(
                      [
                        if (item.detail != null) item.detail!,
                        if (item.isMandatory) S.handoverMandatory,
                        if (item.acknowledged)
                          '${S.handoverAckedAt}: ${item.acknowledgedAt ?? '-'}',
                      ].join(' · '),
                    ),
                    trailing: item.acknowledged
                        ? const Icon(Icons.check_circle, color: Colors.green)
                        : IconButton(
                            icon: const Icon(Icons.done_all),
                            tooltip: S.handoverAckItem,
                            onPressed: () async {
                              try {
                                await ref
                                    .read(handoverDetailProvider(handoverId)
                                        .notifier)
                                    .ackItem(item.itemId);
                                if (context.mounted) {
                                  showAppSnackBar(
                                      context, S.handoverItemAcked);
                                }
                              } catch (e) {
                                if (context.mounted) {
                                  showErrorSnackBar(context, e);
                                }
                              }
                            },
                          ),
                  ),
                ),
              const SizedBox(height: 24),
              FilledButton(
                onPressed: pendingItems > 0
                    ? null
                    : () async {
                        try {
                          await ref
                              .read(handoverDetailProvider(handoverId)
                                  .notifier)
                              .ackWhole();
                          if (context.mounted) {
                            showAppSnackBar(context, S.handoverWholeAcked);
                          }
                        } catch (e) {
                          if (context.mounted) {
                            showErrorSnackBar(context, e);
                          }
                        }
                      },
                child: Text(
                  pendingItems > 0
                      ? '${S.handoverAckWhole} ($pendingItems ${S.handoverPendingItems})'
                      : S.handoverAckWhole,
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}
