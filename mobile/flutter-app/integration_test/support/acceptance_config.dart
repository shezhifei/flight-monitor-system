/// Shared runtime configuration for the Flutter acceptance suite.
///
/// Everything here comes from `--dart-define`, never from a literal inside a
/// test file. The suite talks to a real backend, so it must be repointable
/// without editing test source -- the previous
/// `login(username: 'admin', password: 'admin123')` literals (D-36) made the
/// acceptance run bind to one seeded account on one host.
///
/// Example:
///
/// ```sh
/// flutter test integration_test/dispatch_acceptance_test.dart \
///   --dart-define=FMS_TEST_BASE_URL=http://10.0.2.2:8000 \
///   --dart-define=FMS_TEST_USERNAME=dispatch_ci \
///   --dart-define=FMS_TEST_PASSWORD=<from your secret store>
/// ```
///
/// The defaults are the seeded local development account and the Android
/// emulator loopback host, so an offline run against a locally started backend
/// keeps working with no flags at all.
///
/// SECURITY NOTE: `--dart-define` values are compiled into the app binary, not
/// read at runtime. Use them for a disposable local/CI test identity only, and
/// never inject a real production credential this way.
library;

/// Backend base URL. `http://10.0.2.2` is the host loopback as seen from the
/// Android emulator.
const String kAcceptanceBaseUrl = String.fromEnvironment(
  'FMS_TEST_BASE_URL',
  defaultValue: 'http://10.0.2.2:8000',
);

/// Acceptance login identity.
const String kAcceptanceUsername = String.fromEnvironment(
  'FMS_TEST_USERNAME',
  defaultValue: 'admin',
);

/// Acceptance login secret. Kept out of logs -- see
/// [describeAcceptanceTarget].
const String kAcceptancePassword = String.fromEnvironment(
  'FMS_TEST_PASSWORD',
  defaultValue: 'admin123',
);

/// Whether [kAcceptancePassword] is the compiled-in local default rather than
/// an injected value.
const bool kAcceptancePasswordIsDefault =
    kAcceptancePassword == 'admin123';

/// One-line description of the acceptance target, safe to print: it never
/// echoes the password.
String describeAcceptanceTarget() =>
    'acceptance target base=$kAcceptanceBaseUrl user=$kAcceptanceUsername '
    'credentialSource=${kAcceptancePasswordIsDefault ? 'dart-define-default' : 'dart-define-override'}';
