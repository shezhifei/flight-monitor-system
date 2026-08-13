import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../app/constants.dart';
import '../../app/l10n.dart';
import '../../bridge/api/operations.dart';
import '../../providers/operations_provider.dart';

/// 战情中心：按间隔轮询事件流。
class OperationsScreen extends ConsumerStatefulWidget {
  const OperationsScreen({super.key});

  @override
  ConsumerState<OperationsScreen> createState() => _OperationsScreenState();
}

class _OperationsScreenState extends ConsumerState<OperationsScreen> {
  Timer? _poll;
  String? _severityFilter;

  @override
  void initState() {
    super.initState();
    _poll = Timer.periodic(AppConstants.operationsPollInterval, (_) {
      if (!mounted) return;
      ref.read(operationsFeedProvider.notifier).refresh().catchError((_) {});
    });
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final feed = ref.watch(operationsFeedProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text(S.operationsTitle),
        actions: [
          IconButton(
            tooltip: S.retry,
            icon: const Icon(Icons.refresh),
            onPressed: () =>
                ref.read(operationsFeedProvider.notifier).refresh(),
          ),
        ],
      ),
      body: RefreshIndicator(
        onRefresh: () => ref.read(operationsFeedProvider.notifier).refresh(),
        child: feed.when(
          loading: () => const Center(child: CircularProgressIndicator()),
          error: (e, _) => ListView(
            children: [
              const SizedBox(height: 120),
              Center(child: Text('${S.errorPrefix}$e')),
              Center(
                child: FilledButton(
                  onPressed: () =>
                      ref.read(operationsFeedProvider.notifier).refresh(),
                  child: const Text(S.retry),
                ),
              ),
            ],
          ),
          data: (data) {
            final severities = data.severityCounts.keys.toList()..sort();
            final events = data.events.where((e) {
              if (_severityFilter == null) return true;
              return e.severity.toLowerCase() ==
                  _severityFilter!.toLowerCase();
            }).toList();
            return ListView(
              padding: const EdgeInsets.all(16),
              children: [
                if (data.eventTypeCounts.isNotEmpty)
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      for (final e in data.eventTypeCounts.entries)
                        Chip(label: Text('${e.key}: ${e.value}')),
                    ],
                  ),
                const SizedBox(height: 8),
                if (severities.isNotEmpty)
                  Wrap(
                    spacing: 8,
                    children: [
                      FilterChip(
                        label: const Text(S.operationsFilterAll),
                        selected: _severityFilter == null,
                        onSelected: (_) =>
                            setState(() => _severityFilter = null),
                      ),
                      for (final s in severities)
                        FilterChip(
                          label: Text(s),
                          selected: _severityFilter == s,
                          onSelected: (_) =>
                              setState(() => _severityFilter = s),
                        ),
                    ],
                  ),
                const SizedBox(height: 12),
                if (events.isEmpty)
                  const Padding(
                    padding: EdgeInsets.only(top: 80),
                    child: Center(child: Text(S.operationsEmpty)),
                  )
                else
                  for (final e in events) _EventCard(event: e),
              ],
            );
          },
        ),
      ),
    );
  }
}

class _EventCard extends StatelessWidget {
  const _EventCard({required this.event});
  final OperationsEvent event;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Card(
      margin: const EdgeInsets.only(bottom: 8),
      child: ListTile(
        minVerticalPadding: 12,
        leading: Icon(
          Icons.bolt_outlined,
          color: switch (event.severity.toLowerCase()) {
            'critical' || 'high' => scheme.error,
            'medium' || 'warning' => scheme.tertiary,
            _ => scheme.primary,
          },
        ),
        title: Text(event.title),
        subtitle: Text(
          [
            event.eventType,
            event.status,
            if (event.flightId != null) event.flightId!,
            event.occurredAt,
          ].join(' · '),
        ),
        isThreeLine: true,
      ),
    );
  }
}
