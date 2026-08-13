import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:path_provider/path_provider.dart';
import 'package:uuid/uuid.dart';

import '../bridge/api.dart';
import '../bridge/api/session.dart';
import '../bridge/frb_generated.dart';
import 'constants.dart';
import 'token_store.dart';

/// 启动装配结果：init_core + 会话持久化恢复。
class AppBootstrap {
  const AppBootstrap({
    required this.deviceId,
    required this.baseUrl,
    required this.tokenStore,
    required this.restoredLoggedIn,
  });

  /// 设备稳定 ID（X-Operator-Context-Id）：ANDROID_ID，
  /// 读不到时用持久化的 UUID 兜底。
  final String deviceId;

  /// 本次生效的 base_url。
  final String baseUrl;

  final TokenStore tokenStore;

  /// 启动时是否成功恢复了已登录会话。
  final bool restoredLoggedIn;
}

/// 启动流程：
/// `RustLib.init()` → 解析设备 ID → 读 base_url 覆盖（debug）→
/// `init_core(...)` → 读 secure storage 并 `restore_tokens`。
/// 任一步失败一律 fail-closed 当未登录（禁止降级明文）。
class Bootstrapper {
  Bootstrapper._();

  static Future<AppBootstrap> run() async {
    await RustLib.init();

    const secureStorage = FlutterSecureStorage();
    final deviceId = await _resolveDeviceId(secureStorage);
    final tokenStore = SecureTokenStore(secureStorage);

    String? overrideUrl;
    if (!kReleaseMode) {
      overrideUrl = await secureStorage.read(
        key: AppConstants.storageKeyBaseUrlOverride,
      );
      if (overrideUrl != null && overrideUrl.trim().isEmpty) {
        overrideUrl = null;
      }
    }
    final baseUrl = AppConstants.baseUrl(debugOverride: overrideUrl);

    final supportDir = await getApplicationSupportDirectory();
    final dbPath =
        '${supportDir.path}${Platform.pathSeparator}${AppConstants.offlineDbFileName}';

    await initCore(
      baseUrl: baseUrl,
      allowCleartext: AppConstants.allowCleartext,
      dbPath: dbPath,
      operatorContextId: deviceId,
    );

    var restored = false;
    final bundle = await tokenStore.read();
    if (bundle != null) {
      try {
        await restoreTokens(bundle: bundle);
        restored = true;
      } catch (_) {
        // fail-closed：恢复失败即匿名，同时清掉损坏的持久化。
        await tokenStore.clear();
      }
    }

    return AppBootstrap(
      deviceId: deviceId,
      baseUrl: baseUrl,
      tokenStore: tokenStore,
      restoredLoggedIn: restored,
    );
  }

  /// ANDROID_ID 优先，保证设备 id 稳定；非 Android 或读取失败时用
  /// secure storage 里的持久化 UUID 兜底（保证同一设备稳定）。
  static Future<String> _resolveDeviceId(FlutterSecureStorage storage) async {
    try {
      if (Platform.isAndroid) {
        final info = await DeviceInfoPlugin().androidInfo;
        if (info.id.isNotEmpty) return info.id;
      }
    } catch (_) {
      // fall through to UUID 兜底
    }
    final existing =
        await storage.read(key: AppConstants.storageKeyDeviceIdFallback);
    if (existing != null && existing.isNotEmpty) return existing;
    final generated = const Uuid().v4();
    await storage.write(
      key: AppConstants.storageKeyDeviceIdFallback,
      value: generated,
    );
    return generated;
  }
}
