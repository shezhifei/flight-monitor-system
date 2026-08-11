# Flight Monitor Android Client (Bootstrap)

本目录为移动作业端原生安卓首批可运行骨架，当前已打通：

- 登录：`/api/v2/auth/login`
- 续签：`/api/v2/auth/refresh`
- 当前用户：`/api/v2/auth/me`
- 心跳：`/api/v2/auth/heartbeat`
- SSE Token：`/api/v2/auth/sse-token`
- 移动工作台：`/api/v2/mobile/workbench`
- 设备注册/心跳/注销：`/api/v2/mobile/devices/*`
- 派工主链路：`/api/v2/dispatch/orders/my/assigned` + `/api/v2/dispatch-orders/{id}/accept|checkin|start|complete|report-issue`
- 离线补传：`/api/v2/dispatch-orders/mobile/sync/actions`
- 派工群聊：`/api/v2/dispatch-chat/groups|groups/{id}/messages|groups/{id}/read|stream`
- 通知回执：`/api/v2/notifications|{id}/read|{id}/ack|read-all|stream`
- 战情聚合：`/api/v2/mobile/operations/events`
- 附件上传：`/api/v2/mobile/uploads`
- 交接班：`/api/v2/shift-handovers|{id}|{id}/items/{itemId}/ack|{id}/ack`
- 安全门禁：`/api/v2/dispatch-orders/{id}/safety-checklist|{id}/safety-checklist/items/{itemCode}`

## 版本参数

- `minSdk = 23`（Android 6.0+；EncryptedSharedPreferences / Keystore）
- `targetSdk = 35`（Android 15；Play 要求普通应用至少 target API 35）
- `compileSdk = 35`
- release 仅 HTTPS；仅 debug 允许 cleartext

## 本地启动

1. 用 Android Studio 打开 `android/`。
2. 同步 Gradle。
3. 修改 `android/app/build.gradle.kts` 中 `BuildConfig.API_BASE_URL`：
   - 模拟器访问本机后端默认使用 `http://10.0.2.2:8000/`
4. 运行 `app` 模块。

## 关键流程

- `LauncherActivity`：本地 token 检查 → refresh（如需）→ `me` → 注册设备 → 首次心跳 → 进入工作台
- `LoginActivity`：账号密码登录后自动做 `me/sse-token/设备注册/心跳`
- `WorkbenchActivity`：加载聚合工作台，每 60 秒发送 `auth+device` 心跳并尝试离线补传
- `DispatchActionsActivity`：工单拉取、接单/签到/开工/完工/异常上报、手动补传离线动作队列；支持从系统文件选择附件并上传并写入 `attachments`
- `CollaborationActivity`：群聊分组/消息读取与发送、通知列表/已读/确认/拒绝、双 SSE 实时刷新（chat + notifications）
- `OperationsCenterActivity`：移动战情聚合（事件列表、事件类型/级别过滤、limit 控制、45 秒自动刷新）
- `ShiftHandoverActivity`：交接班列表、详情、条目签收、整单签收
- `SafetyChecklistActivity`：安全门禁检查项状态查看与 pass/fail/na 提交

## 派工闭环说明

- 离线场景下，动作会进入本地队列（`SharedPreferences`）并生成 `client_action_id`。
- 手动点击“补传离线队列”会调用 `/mobile/sync/actions`，后端返回 `applied/duplicate/failed` 后本地清理已成功动作。
- 登录成功、冷启动进入工作台、工作台心跳周期都会尝试自动补传，减少现场补单操作。

## 群聊与通知实时协同

- 客户端通过 `/api/v2/auth/sse-token` 维护 `sse_token`，并以 query 方式连接：
  - `/api/v2/dispatch-chat/stream`
  - `/api/v2/notifications/stream`
- 收到非 heartbeat 事件后，移动端做 1.2 秒防抖刷新，保证消息与通知状态回补。
- 流断开后 5 秒自动重连，适配弱网与后台切前台场景。
- 协同页优先按事件类型做本地增量更新（`chat_message`、`chat_read_synced`、`user_notification`），未知事件再降级触发小范围刷新。

## 战情/交接/门禁入口

- `WorkbenchActivity` 已新增“战情中心 / 交接班 / 安全门禁”入口。
- `DispatchActionsActivity` 新增“安全门禁检查”快捷入口，自动回填当前工单ID。
- 安全门禁状态会显示 `ready` 与必填完成度；后端在完工动作中执行门禁校验，不满足时阻止完工。
- 客户端在“完工”前会先做一次门禁预校验，提前提示待补项，减少无效提交。
- 派工动作收到 HTTP 错误时会尽量解析后端 `detail` 返回（含门禁待补项）并展示可操作信息。
- 通知页现已显式展示 `origin_type/origin_label/receipt_required/ack_status`，并支持通知详情页与回执组详情页。
- 群聊页会根据群组 `read_only` 状态禁用输入与发送，系统消息以独立样式显示，不与普通文本混淆。
- 派工页会基于 `origin_type/origin_label/notification_receipt_summary` 展示当前工单来源与回执汇总。

## UI/UX 优化基线（2026-03）

- 主题色板统一为运营看板风格（`#3B82F6 / #60A5FA / #F97316 / #F8FAFC`），并新增状态色（成功/警告/错误）。
- 所有按钮统一最小触控高度 `48dp`、圆角 `14dp`，满足移动端可点按区域与间距要求。
- 关键信息区统一卡片化（`bg_card_surface`）与状态条（`bg_status_*`），提升弱网和高压场景可读性。
- 状态文案统一通过 `renderStatus(...)` 自动映射为信息/成功/警告/错误视觉反馈。
- 关键页面进一步加入“副标题 + 分区眉题 + 主次按钮层级”，降低长表单感并强化主操作路径。
- 登录、工作台、派工、协同、战情、交接班、安全门禁均已纳入同一套视觉语言与操作层级。

## 安全存储策略

- **minSdk 23+**：`TokenStorage` **仅**使用 `EncryptedSharedPreferences`（Android Keystore AES-256）。
- **Fail-closed**：Keystore / 加密存储不可用时抛出 `SecureTokenStorageException`，**绝不**降级到明文 `SharedPreferences`。
- 启动时清除历史明文 prefs 文件 `mobile_auth_tokens`（不迁移旧 token，需重新登录）。
- 按“最小字段持久化”仅保存 access / refresh / SSE token、过期时间与 session_secret。
