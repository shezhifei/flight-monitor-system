import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';

import '../../app/constants.dart';
import '../../app/l10n.dart';
import '../../bridge/api/dispatch.dart';
import '../../providers/dispatch_provider.dart';
import '../../shared/widgets/snackbar.dart';
import 'status_label.dart';

/// 后端时间字段是 `chrono::DateTime<Utc>`，只接受 RFC3339 带 `Z` 的 UTC
/// 时间（实测无时区字符串 422）。统一按 UTC 渲染。
final _timeFormat = DateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'");

/// 派工页：my/assigned 列表 + 状态机动作按钮 +
/// ETA 对话框 + 问题上报（附件上传）+ 完工门禁预校验。
class DispatchScreen extends ConsumerStatefulWidget {
  const DispatchScreen({super.key});

  @override
  ConsumerState<DispatchScreen> createState() => _DispatchScreenState();
}

class _DispatchScreenState extends ConsumerState<DispatchScreen> {
  DispatchOrder? _selected;
  bool _busy = false;

  Future<void> _refresh() =>
      ref.read(dispatchOrdersProvider.notifier).refresh();

  Future<void> _runAction(
    DispatchActionId action,
    Map<String, dynamic> payload,
  ) async {
    final order = _selected;
    if (order == null || _busy) return;
    setState(() => _busy = true);
    try {
      final feedback = await ref
          .read(dispatchOrdersProvider.notifier)
          .execute(order.id, action, payload);
      if (!mounted) return;
      showAppSnackBar(
        context,
        feedback == DispatchActionFeedback.queued
            ? S.actionQueuedOffline
            : S.actionSent,
        isError: feedback == DispatchActionFeedback.queued,
      );
    } catch (e) {
      if (mounted) showErrorSnackBar(context, e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// 时间手输对话框；预填值按 UTC
  /// 渲染，与 `_timeFormat` 的 RFC3339 `Z` 后缀一致。
  Future<String?> _askTime(String title, DateTime initial) {
    final controller =
        TextEditingController(text: _timeFormat.format(initial.toUtc()));
    return showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(title),
        content: TextField(
          controller: controller,
          decoration: const InputDecoration(hintText: S.etaDialogHint),
          autofocus: true,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext),
            child: const Text(S.cancel),
          ),
          FilledButton(
            onPressed: () =>
                Navigator.pop(dialogContext, controller.text.trim()),
            child: const Text(S.confirm),
          ),
        ],
      ),
    );
  }

  Future<void> _onAction(DispatchActionId action) async {
    switch (action) {
      case DispatchActionId.accept:
        await _runAction(action, {'note': null});
      case DispatchActionId.checkIn:
        await _runAction(action, {'note': null});
      case DispatchActionId.checkOut:
        await _runAction(action, {'note': null});
      case DispatchActionId.start:
        await _runAction(action, {'notes': null});
      case DispatchActionId.etaReport:
        final time = await _askTime(
          S.etaDialogTitle,
          DateTime.now().add(const Duration(minutes: 30)),
        );
        if (time == null || time.isEmpty) return;
        await _runAction(action, {
          'estimated_completion_time': time,
          'note': null,
        });
      case DispatchActionId.complete:
        await _completeWithGate();
    }
  }

  /// 完工门禁预校验：
  /// 先拉安全检查清单，未通过则禁止提交并引导去清单页。
  Future<void> _completeWithGate() async {
    final order = _selected;
    if (order == null || _busy) return;
    setState(() => _busy = true);
    try {
      final checklist = await ref.read(safetyChecklistProvider(order.id).future);
      if (!mounted) return;
      if (checklist.enforced && !checklist.ready) {
        showAppSnackBar(context, S.completeBlocked, isError: true);
        context.push('/dispatch/${order.id}/checklist');
        return;
      }
      final time = await _askTime(S.completeDialogTitle, DateTime.now());
      if (time == null || time.isEmpty) return;
      await _runAction(DispatchActionId.complete, {
        'actual_end_time': time,
        'completion_notes': null,
        'issues': <String>[],
      });
    } catch (e) {
      if (mounted) showErrorSnackBar(context, e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _reportIssue() async {
    final order = _selected;
    if (order == null || _busy) return;
    final titleController = TextEditingController();
    final descController = TextEditingController();
    final attachments = <String>[];
    var uploading = false;

    final submitted = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (dialogContext, setDialogState) => AlertDialog(
          title: const Text(S.issueDialogTitle),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  controller: titleController,
                  decoration:
                      const InputDecoration(hintText: S.issueTitleHint),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: descController,
                  decoration:
                      const InputDecoration(hintText: S.issueDescriptionHint),
                  maxLines: 3,
                ),
                const SizedBox(height: 12),
                Row(
                  children: [
                    TextButton.icon(
                      onPressed: uploading
                          ? null
                          : () async {
                              final picked =
                                  await FilePicker.platform.pickFiles();
                              final path = picked?.files.single.path;
                              if (path == null) return;
                              setDialogState(() => uploading = true);
                              try {
                                final asset = await uploadAttachment(
                                  path: path,
                                  category:
                                      AppConstants.uploadCategoryDispatchIssue,
                                );
                                setDialogState(() =>
                                    attachments.add(asset.attachmentUrl));
                              } catch (e) {
                                if (dialogContext.mounted) {
                                  showErrorSnackBar(dialogContext, e);
                                }
                              } finally {
                                setDialogState(() => uploading = false);
                              }
                            },
                      icon: const Icon(Icons.attach_file),
                      label: Text(uploading
                          ? S.issueAttachmentUploading
                          : S.issueAddAttachment),
                    ),
                    if (attachments.isNotEmpty)
                      Text('×${attachments.length}'),
                  ],
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, false),
              child: const Text(S.cancel),
            ),
            FilledButton(
              onPressed: uploading
                  ? null
                  : () => Navigator.pop(dialogContext, true),
              child: const Text(S.confirm),
            ),
          ],
        ),
      ),
    );
    if (submitted != true) return;
    final title = titleController.text.trim();
    if (title.isEmpty) return;

    setState(() => _busy = true);
    try {
      final feedback = await ref
          .read(dispatchOrdersProvider.notifier)
          .reportIssue(
            order.id,
            title: title,
            description: descController.text.trim().isEmpty
                ? null
                : descController.text.trim(),
            attachments: attachments,
          );
      if (!mounted) return;
      showAppSnackBar(
        context,
        feedback == DispatchActionFeedback.queued
            ? S.actionQueuedOffline
            : S.actionSent,
        isError: feedback == DispatchActionFeedback.queued,
      );
    } catch (e) {
      if (mounted) showErrorSnackBar(context, e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final orders = ref.watch(dispatchOrdersProvider);
    return Scaffold(
      appBar: AppBar(title: const Text(S.navDispatch)),
      body: orders.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text('${S.errorPrefix}$e'),
              const SizedBox(height: 12),
              FilledButton(onPressed: _refresh, child: const Text(S.retry)),
            ],
          ),
        ),
        data: (list) {
          if (list.isEmpty) {
            return RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(children: const [
                SizedBox(height: 120),
                Center(child: Text(S.dispatchEmpty)),
              ]),
            );
          }
          final selected = _selected ?? list.first;
          return Row(
            children: [
              SizedBox(
                width: 320,
                child: RefreshIndicator(
                  onRefresh: _refresh,
                  child: ListView.builder(
                    itemCount: list.length,
                    itemBuilder: (_, i) {
                      final order = list[i];
                      return Card(
                        margin: const EdgeInsets.symmetric(
                            horizontal: 8, vertical: 4),
                        child: ListTile(
                          selected: order.id == selected.id,
                          title:
                              Text('${order.flightId} · ${order.stepCode ?? '-'}'),
                          subtitle: Text(order.originLabel),
                          trailing: StatusChip(status: order.status),
                          onTap: () => setState(() => _selected = order),
                        ),
                      );
                    },
                  ),
                ),
              ),
              const VerticalDivider(width: 1),
              Expanded(child: _detail(selected)),
            ],
          );
        },
      ),
    );
  }

  Widget _detail(DispatchOrder order) {
    final actions = dispatchActionsForStatus(order.status);
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text('${order.flightId} · ${order.stepCode ?? '-'}',
            style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: 8),
        Row(children: [StatusChip(status: order.status)]),
        const SizedBox(height: 8),
        Text([
          if (order.terminal != null) order.terminal!,
          if (order.standId != null) order.standId!,
          if (order.gate != null) order.gate!,
        ].join(' ')),
        if (order.estimatedCompletionTime != null)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Text('${S.dispatchActionEtaReport}: '
                '${order.estimatedCompletionTime}'),
          ),
        const Divider(height: 32),
        if (_busy)
          const Center(
            child: Padding(
              padding: EdgeInsets.all(8),
              child: CircularProgressIndicator(),
            ),
          )
        else ...[
          for (final action in actions)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: FilledButton(
                onPressed: () => _onAction(action),
                child: Text(_actionLabel(action)),
              ),
            ),
          if (safetyChecklistVisibleForStatus(order.status))
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: OutlinedButton.icon(
                onPressed: () =>
                    context.push('/dispatch/${order.id}/checklist'),
                icon: const Icon(Icons.checklist),
                label: const Text(S.dispatchSafetyChecklist),
              ),
            ),
          if (reportIssueVisibleForStatus(order.status))
            OutlinedButton.icon(
              onPressed: _reportIssue,
              icon: const Icon(Icons.report_problem_outlined),
              label: const Text(S.dispatchActionReportIssue),
            ),
        ],
      ],
    );
  }

  String _actionLabel(DispatchActionId action) => switch (action) {
        DispatchActionId.accept => S.dispatchActionAccept,
        DispatchActionId.checkIn => S.dispatchActionCheckIn,
        DispatchActionId.checkOut => S.dispatchActionCheckOut,
        DispatchActionId.start => S.dispatchActionStart,
        DispatchActionId.complete => S.dispatchActionComplete,
        DispatchActionId.etaReport => S.dispatchActionEtaReport,
      };
}
