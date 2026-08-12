# Mobile release 构建记录

## 配置

```powershell
cd mobile\flutter-app
flutter build apk --release --dart-define=API_BASE_URL=https://<prod-host>
```

- 签名：复制 `android/key.properties.example` → `android/key.properties`（gitignore）
- 未配置 keystore 时 release 回退 debug 签名（仅 CI/本地，不可上架）
- debug：`http://10.0.2.2:8000`，允许明文（debug manifest）
- release：强制 https base_url（`AppConstants` assert）

## 实测（2026-08-12）

| 项 | 值 |
|----|-----|
| 命令 | `flutter build apk --release --dart-define=API_BASE_URL=https://example.invalid` |
| 产物 | `mobile/flutter-app/build/app/outputs/flutter-apk/app-release.apk` |
| 包名 | `com.flightmonitor.mobile` |
| 体积 | **69.85 MB**（73 247 200 bytes） |
| minSdk / targetSdk | 24 / 36 |
| versionName | 1.0.0 |
| 模拟器安装 | `emulator-5554` → **Success**（`adb install -r`） |
| primaryCpuAbi（模拟器） | x86_64（release APK 含多 ABI） |
| CI | Mobile workflow `flutter build apk --release` **pass**；artifact `app-release-apk` |

明文策略：release 构建注入 https；debug 变体才允许 cleartext。生产上架前必须配置真实 keystore，勿使用 debug 签名。
