import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../app/l10n.dart';
import '../../bridge/api/dispatch.dart';
import '../../providers/session_provider.dart';
import '../../providers/settings_provider.dart';
import '../../shared/widgets/snackbar.dart';

/// 设置页（plan §5 SettingsScreen）：base_url 展示/debug 覆盖、
/// 设备 ID、离线补传、退出登录。
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  Future<void> _syncNow(BuildContext context, WidgetRef ref) async {
    try {
      final summary = await syncOfflineActions();
      ref.read(pendingSyncCountProvider.notifier).set(summary.remaining);
      if (context.mounted) {
        showAppSnackBar(
          context,
          '${S.settingsSyncNow}: '
          '${summary.applied}✓ ${summary.duplicates}= ${summary.failed}✗',
        );
      }
    } catch (e) {
      if (context.mounted) showErrorSnackBar(context, e);
    }
  }

  Future<void> _editBaseUrl(BuildContext context, WidgetRef ref) async {
    final controller =
        TextEditingController(text: ref.read(baseUrlProvider));
    final saved = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text(S.settingsBaseUrl),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(controller: controller, autofocus: true),
            const SizedBox(height: 8),
            Text(S.settingsBaseUrlDebugOnly,
                style: Theme.of(dialogContext).textTheme.labelSmall),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text(S.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text(S.save),
          ),
        ],
      ),
    );
    if (saved != true) return;
    await saveBaseUrlOverride(controller.text);
    if (context.mounted) showAppSnackBar(context, S.settingsBaseUrlSaved);
  }

  Future<void> _confirmLogout(BuildContext context, WidgetRef ref) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        content: const Text(S.settingsLogoutConfirm),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text(S.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text(S.confirm),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    await performLogout(ref);
    // session_state 流会驱动路由守卫跳 /login。
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final baseUrl = ref.watch(baseUrlProvider);
    final deviceId = ref.watch(deviceIdProvider);
    final pendingSync = ref.watch(pendingSyncCountProvider);

    return Scaffold(
      appBar: AppBar(title: const Text(S.settingsTitle)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          ListTile(
            leading: const Icon(Icons.dns_outlined),
            title: const Text(S.settingsBaseUrl),
            subtitle: Text(baseUrl),
            trailing: kReleaseMode
                ? null
                : IconButton(
                    icon: const Icon(Icons.edit_outlined),
                    onPressed: () => _editBaseUrl(context, ref),
                  ),
          ),
          ListTile(
            leading: const Icon(Icons.phone_android_outlined),
            title: const Text(S.settingsDeviceId),
            subtitle: Text(deviceId),
          ),
          ListTile(
            leading: const Icon(Icons.sync_outlined),
            title: const Text(S.settingsPendingSync),
            subtitle: Text('$pendingSync'),
            trailing: IconButton(
              icon: const Icon(Icons.sync),
              onPressed: () => _syncNow(context, ref),
            ),
          ),
          const Divider(height: 32),
          FilledButton.tonalIcon(
            onPressed: () => _confirmLogout(context, ref),
            icon: const Icon(Icons.logout),
            label: const Text(S.settingsLogout),
          ),
        ],
      ),
    );
  }
}
