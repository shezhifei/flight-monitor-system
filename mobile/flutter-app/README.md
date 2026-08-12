# Flight Monitor Mobile（Flutter + Rust）

Android 客户端重构：`mobile/flutter-app`（UI）+ `mobile/core`（Rust 签名/会话/API/SSE/离线队列）。

## 结构

| 路径 | 说明 |
|------|------|
| `lib/app/` | 路由、主题（M3）、常量、文案、bootstrap |
| `lib/bridge/` | flutter_rust_bridge 生成物（勿手改） |
| `lib/features/` | 登录 / 工作台 / 派工 / 消息 / 通知 / 交接 / 事项 / 战情 / 设置 |
| `lib/providers/` | Riverpod 状态 + SSE demux |
| `../core/crates/mobile-core` | 纯 Rust，零 frb 依赖 |
| `../core/crates/mobile-ffi` | frb 出口 façade |

旧 Kotlin 工程已归档到 `frontend/backup/android-legacy/`（只读对拍参考）。

## 开发

```powershell
$env:PATH = "C:\flutter\bin;$env:PATH"
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_NDK_HOME = "$env:LOCALAPPDATA\Android\Sdk\ndk\29.0.14206865"

# Rust
cargo test --manifest-path ../core/Cargo.toml
cargo clippy --manifest-path ../core/Cargo.toml --all-targets -- -D warnings

# Dart
cd mobile/flutter-app
flutter pub get
flutter analyze
flutter test
flutter run -d emulator-5554
```

frb 重新生成（改 FFI 后）：

```powershell
flutter_rust_bridge_codegen generate   # 在 flutter-app/ 下，配置见 flutter_rust_bridge.yaml
```

## 后端联调

- 默认 debug：`http://10.0.2.2:8000`（模拟器访问宿主）
- 启动后端：`.\scripts\fms.ps1 -Command start -Runtime host -SkipBuild -SkipMigrations`
- 账号：`admin` / `admin123`

## Release 构建

```powershell
# 生产 base_url 必须 https（AppConstants 强制）
flutter build apk --release --dart-define=API_BASE_URL=https://api.example.com

# 签名：复制 android/key.properties.example → android/key.properties 并配置 keystore
# 未配置时 release 使用 debug 签名（仅本地/CI 方便，不可上架）
```

## 计划与交接

- 执行计划：`docs/plans/android-flutter-rust-rebuild-plan.md`
- 交接快照：`docs/plans/android-flutter-rust-rebuild-handoff.md`
- 34 端点回归清单：`docs/plans/android-mobile-endpoint-checklist.md`
- 推送通道评估：`docs/plans/android-mobile-push-channel-eval.md`
