# 移动端（Flutter + Rust）

| 路径 | 说明 |
|------|------|
| `mobile/flutter-app/` | Flutter UI（Android） |
| `mobile/core/crates/mobile-core/` | 纯 Rust 业务（零 frb） |
| `mobile/core/crates/mobile-ffi/` | flutter_rust_bridge 出口 |
| `legacy/android-kotlin/` | 旧 Kotlin App 归档（只读） |
| `.github/workflows/mobile.yml` | CI：test / analyze / debug+release APK |

## 文档

| 文件 | 内容 |
|------|------|
| [endpoint-checklist.md](./endpoint-checklist.md) | 34+ 端点回归清单 |
| [push-channel-eval.md](./push-channel-eval.md) | FCM/厂商推送评估（不实现） |
| [release-notes.md](./release-notes.md) | release 构建与体积记录 |

本机写路径解阻（不改后端）：`scripts/mobile/apply_local_write_paths.ps1`。  
双端实时：`flutter test integration_test/dual_client_realtime_test.dart -d emulator-5554`。  
SSE 重连：`scripts/mobile/run_sse_reconnect.ps1`。

## 约束

- `mobile-core` 禁止依赖 flutter_rust_bridge
- release `API_BASE_URL` 必须 `https://`（`--dart-define`）
- token/secret 不进日志
