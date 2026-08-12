import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../app/l10n.dart';
import '../../bridge/api/business_case.dart';
import '../../providers/business_case_provider.dart';
import '../../shared/widgets/snackbar.dart';

/// 新建事项 / 启动工作流（plan §5 BusinessCaseEditorScreen）。
class BusinessCaseEditorScreen extends ConsumerStatefulWidget {
  const BusinessCaseEditorScreen({super.key});

  @override
  ConsumerState<BusinessCaseEditorScreen> createState() =>
      _BusinessCaseEditorScreenState();
}

class _BusinessCaseEditorScreenState
    extends ConsumerState<BusinessCaseEditorScreen> {
  final _flightId = TextEditingController();
  final _description = TextEditingController();
  BusinessCaseType? _type;
  var _startWorkflow = true;
  var _busy = false;

  @override
  void dispose() {
    _flightId.dispose();
    _description.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final type = _type;
    final flight = _flightId.text.trim();
    final desc = _description.text.trim();
    if (type == null || flight.isEmpty || desc.isEmpty) {
      showAppSnackBar(context, S.businessCaseFormRequired, isError: true);
      return;
    }
    setState(() => _busy = true);
    try {
      if (_startWorkflow) {
        final result = await startCaseWorkflow(
          templateCode: type.code,
          flightId: flight,
          description: desc,
        );
        final caseId = result.businessCase?.caseId;
        if (!mounted) return;
        showAppSnackBar(context, S.businessCaseWorkflowStarted);
        if (caseId != null) {
          context.go('/business-cases/$caseId');
        } else {
          context.pop();
          await ref.read(businessCasesProvider.notifier).refresh();
        }
      } else {
        final created = await createBusinessCase(
          caseType: type.code,
          flightId: flight,
          description: desc,
          visibilityScope: type.visibilityScope,
        );
        if (!mounted) return;
        showAppSnackBar(context, S.actionSent);
        context.go('/business-cases/${created.caseId}');
      }
    } catch (e) {
      if (mounted) showErrorSnackBar(context, e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final types = ref.watch(businessCaseTypesProvider);

    return Scaffold(
      appBar: AppBar(title: const Text(S.businessCaseCreate)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          types.when(
            loading: () => const LinearProgressIndicator(),
            error: (e, _) => Text('${S.errorPrefix}$e'),
            data: (items) => DropdownButtonFormField<BusinessCaseType>(
              // ignore: deprecated_member_use
              value: _type,
              decoration: const InputDecoration(
                labelText: S.businessCaseType,
                border: OutlineInputBorder(),
              ),
              items: [
                for (final t in items)
                  DropdownMenuItem(value: t, child: Text(t.name)),
              ],
              onChanged: _busy
                  ? null
                  : (v) => setState(() => _type = v),
            ),
          ),
          const SizedBox(height: 16),
          TextField(
            controller: _flightId,
            enabled: !_busy,
            decoration: const InputDecoration(
              labelText: S.businessCaseFlightId,
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 16),
          TextField(
            controller: _description,
            enabled: !_busy,
            maxLines: 4,
            decoration: const InputDecoration(
              labelText: S.businessCaseDescription,
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text(S.businessCaseStartWorkflow),
            subtitle: const Text(S.businessCaseStartWorkflowHint),
            value: _startWorkflow,
            onChanged: _busy
                ? null
                : (v) => setState(() => _startWorkflow = v),
          ),
          const SizedBox(height: 24),
          FilledButton(
            onPressed: _busy ? null : _submit,
            style: FilledButton.styleFrom(
              minimumSize: const Size.fromHeight(48),
            ),
            child: _busy
                ? const SizedBox(
                    width: 22,
                    height: 22,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : Text(_startWorkflow
                    ? S.businessCaseSubmitWorkflow
                    : S.businessCaseSubmitCreate),
          ),
        ],
      ),
    );
  }
}
