import 'package:flutter/material.dart';

import '../../app/l10n.dart';

/// 工单状态中文标签。
String statusLabel(String status) {
  return switch (status.toLowerCase()) {
    'pending' => S.statusPending,
    'assigned' => S.statusAssigned,
    'accepted' => S.statusAccepted,
    'checked_in' => S.statusCheckedIn,
    'in_progress' => S.statusInProgress,
    'completed' => S.statusCompleted,
    'cancelled' => S.statusCancelled,
    _ => status,
  };
}

/// 状态 → (前景, 背景) 颜色（分组语义：
/// pending/assigned=warning 系，进行中=info/primary 系，
/// completed=success 系，cancelled=弱化）。色值取自 ColorScheme，
/// 不硬编码。
(Color, Color) statusColors(ColorScheme scheme, String status) {
  return switch (status.toLowerCase()) {
    'pending' || 'assigned' => (
        scheme.onTertiaryContainer,
        scheme.tertiaryContainer
      ),
    'accepted' || 'checked_in' || 'in_progress' => (
        scheme.onPrimaryContainer,
        scheme.primaryContainer
      ),
    'completed' => (scheme.onSecondaryContainer, scheme.secondaryContainer),
    'cancelled' => (scheme.onSurfaceVariant, scheme.surfaceContainerHighest),
    _ => (scheme.onSurface, scheme.surfaceContainerHighest),
  };
}

/// 状态标签 Chip。
class StatusChip extends StatelessWidget {
  const StatusChip({super.key, required this.status});

  final String status;

  @override
  Widget build(BuildContext context) {
    final (fg, bg) = statusColors(Theme.of(context).colorScheme, status);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Text(
        statusLabel(status),
        style: TextStyle(color: fg, fontSize: 12),
      ),
    );
  }
}
