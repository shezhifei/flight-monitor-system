import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:flight_monitor/bridge/api.dart';
import 'package:flight_monitor/bridge/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('rust lib loads and ping_sign_demo round-trips', (tester) async {
    await RustLib.init();
    final headers = await pingSignDemo(
      method: 'GET',
      uri: '/api/v2/ping',
      body: const [],
      secret: 'deadbeef',
    );
    // GET must use the fixed empty-body hash; signature is hex(SHA-256) = 64 chars.
    expect(
      headers.bodySha256,
      'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    );
    expect(headers.signature.length, 64);
    expect(headers.nonce.length, 32);
  });
}
