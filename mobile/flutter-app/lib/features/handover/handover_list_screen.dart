import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/l10n.dart';
import '../../bridge/api/handover.dart';
import '../../providers/handover_provider.dart';

/// 交接班列表（plan §5 HandoverListScreen）。
class HandoverListScreen extends ConsumerWidget {
  const HandoverListScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final list = ref.watch(handoversProvider);

    return Scaffold(
      appBar: AppBar(title: const Text(S.handoverTitle)),
      body: RefreshIndicator(
        onRefresh: () => ref.read(handoversProvider.notifier).refresh(),
        child: list.when(
          loading: () => const Center(child: CircularProgressIndicator()),
          error: (e, _) => ListView(
            children: [
              const SizedBox(height: 120),
              Center(child: Text('${S.errorPrefix}$e')),
              Center(
                child: FilledButton(
                  onPressed: () =>
                      ref.read(handoversProvider.notifier).refresh(),
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
                  Center(child: Text(S.handoverEmpty)),
                ],
              );
            }
            return ListView.separated(
              itemCount: items.length,
              separatorBuilder: (_, _) => const Divider(height: 1),
              itemBuilder: (context, i) => _Tile(item: items[i]),
            );
          },
        ),
      ),
    );
  }
}

class _Tile extends StatelessWidget {
  const _Tile({required this.item});
  final Handover item;

  @override
  Widget build(BuildContext context) {
    final from = item.fromOperatorLabel ??
        item.fromOperatorName ??
        item.fromUserId;
    final to =
        item.toOperatorLabel ?? item.toOperatorName ?? item.toUserId;
    return ListTile(
      title: Text('${item.shiftDate} · ${item.shiftCode}'),
      subtitle: Text('$from → $to · ${item.status}'),
      trailing: Chip(label: Text(item.riskLevel)),
      onTap: () => context.push('/handover/${item.handoverId}'),
    );
  }
}
