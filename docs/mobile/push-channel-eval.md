# 移动端推送通道评估（仅评估，不实现）

> P3 交付物。当前 App **不实现** FCM/厂商通道；事件以登录后 SSE + 60s 心跳为主。

## 现状

- 实时：`GET /api/v2/sse/stream`（Bearer，防重放跳过），Rust 指数退避 1s→30s，心跳超时 90s。
- 设备注册已带 `push_channel` / `push_token` 可选字段（`MobileDeviceRegisterRequest`），便于日后扩展。
- 工作台 `channel_recommendation` 由后端返回，UI 暂未消费。

## 通道对比

| 通道 | 适用 | 优点 | 风险 / 成本 |
|------|------|------|-------------|
| **FCM** | 海外 / 国内有 Google 服务设备 | 标准、文档全、与 Flutter `firebase_messaging` 成熟 | 国内无 GMS 设备送达率低；需 Firebase 项目与服务账号 |
| **厂商通道**（华为 HMS / 小米 / OPPO / vivo / 荣耀） | 国内主流 ROM | 后台存活与送达率高 | 多 SDK 集成、审核、签名包名绑定；维护成本高 |
| **统一聚合**（如个推 / 极光 / 友盟） | 想一次接入多厂商 | 降低厂商对接面 | 商业授权、数据出境/合规评估、黑盒 |
| **仅 SSE + 前台服务** | 外场作业 App 常亮场景 | 无推送依赖、与现有架构一致 | 杀进程/省电策略下无到达；不适合离线告警 |

## 建议（分阶段）

1. **短期（当前）**：保持 SSE + 本地通知（可选 `flutter_local_notifications` 在前台/连上时提示），不接 FCM。
2. **中期**：若需杀进程到达 → 优先 **FCM 数据消息** 作唤醒信令，payload 仅 `notification_id` / `group_id`，详情仍走 REST（避免推送体泄密）。
3. **国内规模化**：在 FCM 之上叠加 **厂商通道或聚合 SDK**；`push_channel` 上报实际通道名（`fcm`/`hms`/`mi`…），后端按通道路由。
4. **合规**：推送 token 视为 PII 附属；日志禁止打印 token；密钥进 Vault，不进 APK。

## 与后端的接口预留

- 注册：`POST /api/v2/mobile/devices/register` 已有 `push_channel` / `push_token`。
- 建议后端后续：token 刷新独立接口、按用户/角色主题订阅、静默数据消息与展示通知分流。  
  **本次不改后端**（红线）。

## 结论

P3 **不实现**推送。上线默认依赖 SSE；需要离线到达时另开专项，按「FCM 信令 → 国内厂商补齐」路径评估商务与工时。
