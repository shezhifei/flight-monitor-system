import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../app/l10n.dart';
import '../../bridge/api/dispatch.dart';
import '../../providers/dispatch_provider.dart';
import '../../shared/widgets/snackbar.dart';
import 'status_label.dart';

/// 安全检查清单页：逐项提交
/// pass/fail/na，提交后刷新；顶部显示 ready/enforced 门禁状态。
class SafetyChecklistScreen extends ConsumerWidget {
  const SafetyChecklistScreen({super.key, required this.orderId});

  final String orderId;

  Future<void> _submit(
    BuildContext context,
    WidgetRef ref,
    ChecklistItem item,
    String result,
  ) async {
    try {
      await submitChecklistItem(
        orderId: orderId,
        itemCode: item.itemCode,
        result: result,
      );
      ref.invalidate(safetyChecklistProvider(orderId));
      if (context.mounted) showAppSnackBar(context, S.actionSent);
    } catch (e) {
      if (context.mounted) showErrorSnackBar(context, e);
    }
  }

  Future<void> _askResult(
      BuildContext context, WidgetRef ref, ChecklistItem item) async {
    final result = await showModalBottomSheet<String>(
      context: context,
      builder: (sheetContext) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.check_circle_outline),
              title: const Text(S.checklistResultPass),
              onTap: () => Navigator.pop(sheetContext, 'pass'),
            ),
            ListTile(
              leading: const Icon(Icons.cancel_outlined),
              title: const Text(S.checklistResultFail),
              onTap: () => Navigator.pop(sheetContext, 'fail'),
            ),
            if (item.allowNa)
              ListTile(
                leading: const Icon(Icons.remove_circle_outline),
                title: const Text(S.checklistResultNa),
                onTap: () => Navigator.pop(sheetContext, 'na'),
              ),
          ],
        ),
      ),
    );
    if (result == null || !context.mounted) return;
    await _submit(context, ref, item, result);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final checklist = ref.watch(safetyChecklistProvider(orderId));
    return Scaffold(
      appBar: AppBar(title: const Text(S.checklistTitle)),
      body: checklist.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('${S.errorPrefix}$e'),
              const SizedBox(height: 12),
              FilledButton(
                onPressed: () =>
                    ref.invalidate(safetyChecklistProvider(orderId)),
                child: const Text(S.retry),
              ),
            ],
          ),
        ),
        data: (data) => ListView(
          padding: const EdgeInsets.all(16),
          children: [
            Card(
              child: ListTile(
                leading: Icon(
                  data.ready ? Icons.verified : Icons.warning_amber,
                  color: data.ready
                      ? Theme.of(context).colorScheme.primary
                      : Theme.of(context).colorScheme.error,
                ),
                title: Text(
                    data.ready ? S.checklistReady : S.checklistNotReady),
                subtitle: Text(
                  '${data.completedRequired}/${data.requiredTotal}'
                  '${data.enforced ? ' · ${S.checklistEnforced}' : ''}',
                ),
              ),
            ),
            const SizedBox(height: 8),
            for (final item in data.items)
              Card(
                child: ListTile(
                  title: Text(item.title),
                  subtitle: Text([
                    item.itemCode,
                    if (item.required_) S.checklistEnforced,
                    if (item.checkedByUsername != null)
                      item.checkedByUsername!,
                  ].join(' · ')),
                  trailing: StatusChip(status: item.status),
                  onTap: () => _askResult(context, ref, item),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
