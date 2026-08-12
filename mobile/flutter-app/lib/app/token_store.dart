import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import '../bridge/api/session.dart';
import 'constants.dart';

/// TokenBundle 持久化抽象（fail-closed：读取失败一律视为未登录，
/// 禁止降级明文存储）。测试用 [InMemoryTokenStore] 覆盖。
abstract class TokenStore {
  Future<TokenBundle?> read();
  Future<void> write(TokenBundle bundle);
  Future<void> clear();
}

/// flutter_secure_storage 实现（Android Keystore 加密）。
class SecureTokenStore implements TokenStore {
  SecureTokenStore([FlutterSecureStorage? storage])
      : _storage = storage ?? const FlutterSecureStorage();

  final FlutterSecureStorage _storage;

  @override
  Future<TokenBundle?> read() async {
    try {
      final raw =
          await _storage.read(key: AppConstants.storageKeyTokenBundle);
      if (raw == null) return null;
      final map = jsonDecode(raw) as Map<String, dynamic>;
      return TokenBundle(
        accessToken: map['access_token'] as String,
        refreshToken: map['refresh_token'] as String,
        sessionSecret: map['session_secret'] as String,
        accessExpireAt: map['access_expire_at'] as int,
      );
    } catch (_) {
      // fail-closed：任何损坏/读取失败都当未登录处理。
      return null;
    }
  }

  @override
  Future<void> write(TokenBundle bundle) async {
    final raw = jsonEncode({
      'access_token': bundle.accessToken,
      'refresh_token': bundle.refreshToken,
      'session_secret': bundle.sessionSecret,
      'access_expire_at': bundle.accessExpireAt.toInt(),
    });
    await _storage.write(key: AppConstants.storageKeyTokenBundle, value: raw);
  }

  @override
  Future<void> clear() async {
    await _storage.delete(key: AppConstants.storageKeyTokenBundle);
  }
}

/// 测试用内存实现。
class InMemoryTokenStore implements TokenStore {
  TokenBundle? value;

  @override
  Future<TokenBundle?> read() async => value;

  @override
  Future<void> write(TokenBundle bundle) async => value = bundle;

  @override
  Future<void> clear() async => value = null;
}
