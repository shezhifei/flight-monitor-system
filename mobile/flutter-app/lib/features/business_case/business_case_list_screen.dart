import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/l10n.dart';
import '../../bridge/api/business_case.dart';
import '../../providers/business_case_provider.dart';

/// 业务事项列表。
class BusinessCaseListScreen extends ConsumerWidget {
  const BusinessCaseListScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final list = ref.watch(businessCasesProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text(S.businessCaseTitle),
        actions: [
          IconButton(
            tooltip: S.businessCaseCreate,
            icon: const Icon(Icons.add),
            onPressed: () => context.push('/business-cases/new'),
          ),
        ],
      ),
      body: RefreshIndicator(
        onRefresh: () => ref.read(businessCasesProvider.notifier).refresh(),
        child: list.when(
          loading: () => const Center(child: CircularProgressIndicator()),
          error: (e, _) => ListView(
            children: [
              const SizedBox(height: 120),
              Center(child: Text('${S.errorPrefix}$e')),
              Center(
                child: FilledButton(
                  onPressed: () =>
                      ref.read(businessCasesProvider.notifier).refresh(),
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
                  Center(child: Text(S.businessCaseEmpty)),
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
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () => context.push('/business-cases/new'),
        icon: const Icon(Icons.add),
        label: const Text(S.businessCaseCreate),
      ),
    );
  }
}

class _Tile extends StatelessWidget {
  const _Tile({required this.item});
  final BusinessCase item;

  @override
  Widget build(BuildContext context) {
    final typeLabel = item.caseTypeName ?? item.caseType;
    return ListTile(
      minVerticalPadding: 12,
      title: Text('${item.flightNo} · $typeLabel'),
      subtitle: Text(
        '${item.description}\n${item.status} · ${item.createdAt}',
        maxLines: 2,
        overflow: TextOverflow.ellipsis,
      ),
      isThreeLine: true,
      trailing: Chip(label: Text(item.status)),
      onTap: () => context.push('/business-cases/${item.caseId}'),
    );
  }
}
