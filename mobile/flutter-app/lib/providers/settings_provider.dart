import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/constants.dart';
import 'session_provider.dart';

/// 设置域：base_url 展示/debug 覆盖、
/// 离线待同步计数。
final baseUrlProvider = Provider<String>(
  (ref) => ref.watch(bootstrapProvider).baseUrl,
);

/// 离线待同步动作数（sync_offline_actions 后更新；workbench 心跳也读它）。
final pendingSyncCountProvider =
    NotifierProvider<MutableNotifier<int>, int>(() => MutableNotifier(0));

/// debug 覆盖 base_url：写 secure storage，下次启动生效；
/// 本实现保存后立即重新装配需要重启，故只持久化并提示重新登录。
Future<void> saveBaseUrlOverride(String url) async {
  const storage = FlutterSecureStorage();
  final trimmed = url.trim();
  if (trimmed.isEmpty) {
    await storage.delete(key: AppConstants.storageKeyBaseUrlOverride);
  } else {
    await storage.write(
      key: AppConstants.storageKeyBaseUrlOverride,
      value: trimmed,
    );
  }
}
