import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../app/bootstrap.dart';
import '../app/token_store.dart';
import '../bridge/api/session.dart';

/// 启动装配（main() 里 override）。
final bootstrapProvider = Provider<AppBootstrap>(
  (ref) => throw UnimplementedError('bootstrapProvider must be overridden'),
);

final tokenStoreProvider = Provider<TokenStore>(
  (ref) => ref.watch(bootstrapProvider).tokenStore,
);

/// 设备稳定 ID（operator_context_id）。
final deviceIdProvider = Provider<String>(
  (ref) => ref.watch(bootstrapProvider).deviceId,
);

/// 简单可变状态（替代 Riverpod 3 移除的 StateProvider）。
class MutableNotifier<T> extends Notifier<T> {
  MutableNotifier(this._initial);

  final T _initial;

  @override
  T build() => _initial;

  void set(T value) => state = value;
}

/// 登录状态（由 Rust 侧 session_state 流驱动，router 守卫读它）。
final loggedInProvider =
    NotifierProvider<MutableNotifier<bool>, bool>(() => MutableNotifier(false));

/// Rust session_state 流（token-free；匿名/Active{access_expire_at}）。
final sessionStateStreamProvider = StreamProvider<SessionState>(
  (ref) => sessionState(),
);

/// 退出登录：Rust 侧清 session（stream 会驱动路由跳 /login），
/// 本地 secure storage 同步清除。
Future<void> performLogout(WidgetRef ref) async {
  try {
    await logout();
  } catch (_) {
    // 服务端登出失败也要清本地（fail-closed 的反向：不让用户卡在假登录态）
  }
  await ref.read(tokenStoreProvider).clear();
}
