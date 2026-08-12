import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:uuid/uuid.dart';

/// Browser-surface HTTP client for dual-client acceptance.
///
/// Matches Vue (`X-Client-Surface: web`, cookie session, anti-replay HMAC).
/// Login is unsigned; subsequent writes are signed with the `session_secret`
/// cookie. Tokens stay in memory for the test process only.
class WebSurfaceClient {
  WebSurfaceClient(this.baseUrl);

  final String baseUrl;
  static const webUa =
      'Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0';

  final HttpClient _http = HttpClient();
  String? sessionSecret;
  String? accessToken;
  String? cookieHeader;

  Future<void> login({
    required String username,
    required String password,
  }) async {
    final uri = Uri.parse('$baseUrl/api/v2/auth/login');
    final req = await _http.postUrl(uri);
    req.headers.set(HttpHeaders.contentTypeHeader, 'application/json');
    req.headers.set(HttpHeaders.userAgentHeader, webUa);
    req.headers.set('X-Client-Surface', 'web');
    req.add(utf8.encode(jsonEncode({
      'username': username,
      'password': password,
    })));
    final resp = await req.close();
    final body = await utf8.decodeStream(resp);
    if (resp.statusCode >= 400) {
      throw HttpException('web login HTTP ${resp.statusCode}: $body');
    }
    final cookies = resp.cookies;
    sessionSecret = _cookie(cookies, 'session_secret');
    accessToken = _cookie(cookies, 'access_token');
    cookieHeader = cookies.map((c) => '${c.name}=${c.value}').join('; ');
    if (sessionSecret == null || sessionSecret!.isEmpty) {
      throw StateError('web login did not set session_secret cookie');
    }
  }

  Future<Map<String, dynamic>> post(
    String path,
    Map<String, dynamic> body,
  ) async {
    return _call('POST', path, body);
  }

  Future<Map<String, dynamic>> get(String path) async {
    return _call('GET', path, null);
  }

  Future<Map<String, dynamic>> _call(
    String method,
    String path,
    Map<String, dynamic>? bodyObj,
  ) async {
    final secret = sessionSecret;
    if (secret == null) {
      throw StateError('web client not logged in');
    }
    final bodyBytes =
        bodyObj == null ? <int>[] : utf8.encode(jsonEncode(bodyObj));
    final bodyHash = sha256.convert(bodyBytes).toString();
    final ts = (DateTime.now().millisecondsSinceEpoch ~/ 1000).toString();
    final nonce = const Uuid().v4().replaceAll('-', '');
    final payload = '$method:$path:$ts:$nonce:$bodyHash';
    final sig = Hmac(sha256, utf8.encode(secret))
        .convert(utf8.encode(payload))
        .toString();

    final uri = Uri.parse('$baseUrl$path');
    final req = method == 'GET'
        ? await _http.getUrl(uri)
        : await _http.postUrl(uri);
    req.headers.set(HttpHeaders.userAgentHeader, webUa);
    req.headers.set('X-Client-Surface', 'web');
    req.headers.set('X-Request-Timestamp', ts);
    req.headers.set('X-Request-Nonce', nonce);
    req.headers.set('X-Request-Body-SHA256', bodyHash);
    req.headers.set('X-Request-Signature', sig);
    if (cookieHeader != null) {
      req.headers.set(HttpHeaders.cookieHeader, cookieHeader!);
    }
    if (bodyObj != null) {
      req.headers.set(HttpHeaders.contentTypeHeader, 'application/json');
      req.add(bodyBytes);
    }
    final resp = await req.close();
    final text = await utf8.decodeStream(resp);
    if (resp.statusCode >= 400) {
      throw HttpException('web $method $path HTTP ${resp.statusCode}: $text');
    }
    if (text.isEmpty) return <String, dynamic>{};
    final decoded = jsonDecode(text);
    if (decoded is Map<String, dynamic>) return decoded;
    return <String, dynamic>{'data': decoded};
  }

  void close() => _http.close(force: true);

  static String? _cookie(List<Cookie> cookies, String name) {
    for (final c in cookies) {
      if (c.name == name) return c.value;
    }
    return null;
  }
}
