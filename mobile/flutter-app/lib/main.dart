import 'dart:async';

import 'package:flutter/material.dart';

import 'bridge/api.dart';
import 'bridge/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(const FlightMonitorApp());
}

class FlightMonitorApp extends StatelessWidget {
  const FlightMonitorApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Flight Monitor',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFF1565C0)),
        useMaterial3: true,
      ),
      home: const P0DemoHome(),
    );
  }
}

/// P0 demo shell: FFI sign demo + SSE demo (plan P0 tasks 4-5).
class P0DemoHome extends StatefulWidget {
  const P0DemoHome({super.key});

  @override
  State<P0DemoHome> createState() => _P0DemoHomeState();
}

class _P0DemoHomeState extends State<P0DemoHome> {
  int _index = 0;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: IndexedStack(
        index: _index,
        children: const [SignDemoScreen(), SseDemoScreen()],
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _index,
        onDestinationSelected: (i) => setState(() => _index = i),
        destinations: const [
          NavigationDestination(icon: Icon(Icons.key), label: '签名 Demo'),
          NavigationDestination(icon: Icon(Icons.stream), label: 'SSE Demo'),
        ],
      ),
    );
  }
}

/// P0 FFI round-trip demo: calls Rust `ping_sign_demo` and shows the four
/// anti-replay signature headers (plan P0 task 4).
class SignDemoScreen extends StatefulWidget {
  const SignDemoScreen({super.key});

  @override
  State<SignDemoScreen> createState() => _SignDemoScreenState();
}

class _SignDemoScreenState extends State<SignDemoScreen> {
  SignatureHeaders? _result;
  String? _error;
  bool _running = false;

  Future<void> _runDemo() async {
    setState(() {
      _running = true;
      _error = null;
    });
    try {
      final headers = await pingSignDemo(
        method: 'POST',
        uri: '/api/v2/dispatch-orders/abc/accept?t=1',
        body: '{"a":1}'.codeUnits,
        secret: 'deadbeef',
      );
      setState(() => _result = headers);
    } catch (e) {
      setState(() => _error = e.toString());
    } finally {
      setState(() => _running = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('FFI 签名 Demo')),
      body: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            FilledButton(
              onPressed: _running ? null : _runDemo,
              child: Text(_running ? '签名中…' : '运行 ping_sign_demo'),
            ),
            const SizedBox(height: 16),
            if (_error != null)
              Text('错误: $_error',
                  style: TextStyle(color: Theme.of(context).colorScheme.error)),
            if (_result != null) ...[
              _kv('X-Request-Timestamp', _result!.timestamp),
              _kv('X-Request-Nonce', _result!.nonce),
              _kv('X-Request-Body-SHA256', _result!.bodySha256),
              _kv('X-Request-Signature', _result!.signature),
            ],
          ],
        ),
      ),
    );
  }

  Widget _kv(String key, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(key, style: const TextStyle(fontWeight: FontWeight.bold)),
          SelectableText(value,
              style: const TextStyle(fontFamily: 'monospace', fontSize: 12)),
        ],
      ),
    );
  }
}

/// P0 SSE demo: connects to the universal `/api/v2/sse/stream` (which
/// auto-includes the caller's notifications + chat topics) with a manually
/// supplied token and prints every event / connection state (plan P0 task 5,
/// endpoint corrected against backend source).
class SseDemoScreen extends StatefulWidget {
  const SseDemoScreen({super.key});

  @override
  State<SseDemoScreen> createState() => _SseDemoScreenState();
}

class _SseDemoScreenState extends State<SseDemoScreen> {
  final _baseUrlController =
      TextEditingController(text: 'http://10.0.2.2:5000');
  final _tokenController = TextEditingController();
  final List<String> _log = [];
  StreamSubscription<SseUpdate>? _subscription;
  String _status = '未连接';

  bool get _connected => _subscription != null;

  void _connect() {
    final token = _tokenController.text.trim();
    if (token.isEmpty) return;
    setState(() {
      _log.clear();
      _status = '连接中…';
    });
    _subscription = notificationsStream(
      baseUrl: _baseUrlController.text.trim(),
      accessToken: token,
    ).listen(
      (update) => setState(() {
        switch (update) {
          case SseUpdate_State(field0: final state):
            switch (state) {
              case SseConnectionState_Connecting():
                _status = '连接中…';
              case SseConnectionState_Connected():
                _status = '已连接';
              case SseConnectionState_Disconnected(reason: final reason):
                _status = '已断开: $reason';
            }
            _log.insert(0, '[state] $_status');
          case SseUpdate_Event(field0: final event):
            _log.insert(0, '[${event.event}] ${event.data}');
        }
      }),
      onError: (Object e) => setState(() {
        _status = '错误: $e';
        _log.insert(0, _status);
      }),
    );
    setState(() {});
  }

  void _disconnect() {
    _subscription?.cancel();
    _subscription = null;
    setState(() => _status = '未连接');
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _baseUrlController.dispose();
    _tokenController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('SSE 通知流 Demo')),
      body: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              controller: _baseUrlController,
              decoration: const InputDecoration(labelText: 'Base URL'),
            ),
            TextField(
              controller: _tokenController,
              decoration:
                  const InputDecoration(labelText: 'Access Token（手动注入）'),
              obscureText: true,
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: FilledButton(
                    onPressed: _connected ? _disconnect : _connect,
                    child: Text(_connected ? '断开' : '连接'),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(child: Text(_status, maxLines: 2)),
              ],
            ),
            const Divider(height: 24),
            Expanded(
              child: ListView.builder(
                itemCount: _log.length,
                itemBuilder: (context, i) => SelectableText(
                  _log[i],
                  style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
