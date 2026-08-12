import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../app/l10n.dart';
import '../../bridge/api/business_case.dart';
import '../../providers/business_case_provider.dart';
import '../../shared/widgets/snackbar.dart';

/// 业务事项详情 + 追加 + 工作流查看（plan §5 BusinessCaseDetailScreen）。
class BusinessCaseDetailScreen extends ConsumerWidget {
  const BusinessCaseDetailScreen({super.key, required this.caseId});

  final String caseId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final async = ref.watch(businessCaseDetailProvider(caseId));

    return Scaffold(
      appBar: AppBar(title: const Text(S.businessCaseDetailTitle)),
      body: async.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('${S.errorPrefix}$e')),
        data: (c) => ListView(
          padding: const EdgeInsets.all(16),
          children: [
            Text('${c.flightNo} · ${c.caseTypeName ?? c.caseType}',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            Text(c.description),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                Chip(label: Text(c.status)),
                Chip(label: Text(c.visibilityScope)),
                if (c.departmentNameSnapshot != null)
                  Chip(label: Text(c.departmentNameSnapshot!)),
              ],
            ),
            const SizedBox(height: 8),
            Text('${S.businessCaseCreatedBy}: ${c.createdBy}'),
            Text('${S.businessCaseCreatedAt}: ${c.createdAt}'),
            if (c.finishedAt != null)
              Text('${S.businessCaseFinishedAt}: ${c.finishedAt}'),
            const SizedBox(height: 16),
            Text(S.businessCaseAppends,
                style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            if (c.appendEntries.isEmpty && c.latestAppend == null)
              const Text(S.businessCaseNoAppends)
            else
              for (final a in _appends(c))
                Card(
                  child: ListTile(
                    title: Text(a.content),
                    subtitle: Text(
                      '${a.submittedOperatorName ?? a.submittedBy} · ${a.appendedAt}',
                    ),
                    trailing: IconButton(
                      tooltip: S.businessCaseAckAppend,
                      icon: const Icon(Icons.done_all),
                      onPressed: () async {
                        try {
                          await ref
                              .read(businessCaseDetailProvider(caseId).notifier)
                              .acknowledgeAppend(a.appendId);
                          if (context.mounted) {
                            showAppSnackBar(context, S.actionSent);
                          }
                        } catch (e) {
                          if (context.mounted) showErrorSnackBar(context, e);
                        }
                      },
                    ),
                  ),
                ),
            const SizedBox(height: 16),
            FilledButton.icon(
              onPressed: () => _appendDialog(context, ref),
              icon: const Icon(Icons.note_add_outlined),
              label: const Text(S.businessCaseAddAppend),
            ),
            const SizedBox(height: 12),
            OutlinedButton.icon(
              onPressed: () => _showWorkflow(context),
              icon: const Icon(Icons.account_tree_outlined),
              label: const Text(S.businessCaseWorkflow),
            ),
          ],
        ),
      ),
    );
  }

  List<BusinessCaseAppend> _appends(BusinessCase c) {
    if (c.appendEntries.isNotEmpty) return c.appendEntries;
    if (c.latestAppend != null) return [c.latestAppend!];
    return const [];
  }

  Future<void> _appendDialog(BuildContext context, WidgetRef ref) async {
    final controller = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text(S.businessCaseAddAppend),
        content: TextField(
          controller: controller,
          maxLines: 4,
          decoration: const InputDecoration(hintText: S.businessCaseAppendHint),
        ),
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
    final text = controller.text.trim();
    if (text.isEmpty) return;
    try {
      await ref.read(businessCaseDetailProvider(caseId).notifier).append(text);
      if (context.mounted) showAppSnackBar(context, S.actionSent);
    } catch (e) {
      if (context.mounted) showErrorSnackBar(context, e);
    }
  }

  Future<void> _showWorkflow(BuildContext context) async {
    try {
      final detail = await caseWorkflow(caseId: caseId);
      if (!context.mounted) return;
      final run = detail.run;
      await showDialog<void>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: const Text(S.businessCaseWorkflow),
          content: run == null
              ? const Text(S.businessCaseNoWorkflow)
              : Text(
                  '${S.businessCaseWorkflowStatus}: ${run.status}\n'
                  'run: ${run.runId}\n'
                  'process: ${run.processInstanceId}\n'
                  'outcome: ${run.outcome ?? '-'}',
                ),
          actions: [
            FilledButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text(S.confirm),
            ),
          ],
        ),
      );
    } catch (e) {
      if (context.mounted) showErrorSnackBar(context, e);
    }
  }
}
