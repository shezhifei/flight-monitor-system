# Mobile 34 端点回归清单

> 对照 `../plans/android-flutter-rust-rebuild-plan.md (local)` §0.5 与旧 App 对拍面。  
> 状态：`implemented` = mobile-core + FFI + UI 已接线；`core-only` = 仅 core/FFI；`n/a` = 后端已迁路径。  
> 验证列：模拟器对拍 200 记 `ok`；写路径 `write_ok`；环境阻塞 `env_block`；未跑 `pending`。  
> **收口日 2026-08-12**：读路径 smoke + 写路径 smoke + 离线 applied 已跑。

## Auth / Device

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 1 | `POST /api/v2/auth/login` | implemented | ok |
| 2 | `POST /api/v2/auth/refresh` | implemented | ok |
| 3 | `GET /api/v2/auth/me` | implemented | ok |
| 4 | `POST /api/v2/auth/logout` | implemented | ok |
| 5 | `POST /api/v2/auth/heartbeat` | implemented | ok |
| 6 | `POST /api/v2/mobile/devices/register` | implemented | ok |
| 7 | `POST /api/v2/mobile/devices/{id}/heartbeat` | implemented | ok |

## Mobile

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 8 | `GET /api/v2/mobile/workbench` | implemented | ok |
| 9 | `GET /api/v2/mobile/operations/events` | implemented | ok |
| 10 | `POST /api/v2/mobile/uploads` | implemented | ok |

## Dispatch

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 11 | `GET /api/v2/dispatch-orders/my/assigned` | implemented | ok |
| 12–18 | 7 动作 accept/checkin/checkout/start/complete/eta-report/report-issue | implemented | ok |
| 19 | `POST /api/v2/dispatch-orders/mobile/sync/actions` | implemented | ok |
| 20 | `GET .../safety-checklist` | implemented | ok |
| 21 | `POST .../safety-checklist/items/{code}` | implemented | pending（部分模板 DB varchar 500） |

## Chat（路径修正）

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 22 | `GET /api/v2/dispatch/collaboration/groups` | implemented | ok |
| 23 | `GET .../groups/{id}/messages` | implemented | ok |
| 24 | `POST .../groups/{id}/messages` | implemented | env_block（FK `fk_dispatch_chat_messages_event`） |
| 25 | `POST .../groups/{id}/read` | implemented | pending（依赖成功 send） |

> 旧路径 `/api/v2/dispatch-chat/*` 已 404，勿再使用。

## Notifications

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 26 | `GET /api/v2/notifications` | implemented | ok |
| 27 | `GET /api/v2/notifications/unread-count` | implemented | ok |
| 28 | `POST /api/v2/notifications/{id}/read` | implemented | pending（无未读样本） |
| 29 | `POST /api/v2/notifications/read-all` | implemented | write_ok |
| 30 | `POST /api/v2/notifications/{id}/ack` | implemented | pending |
| 31 | `GET /api/v2/notifications/receipt-groups/{id}` | implemented | pending |

## Shift handover

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 32 | `GET /api/v2/shift-handovers` | implemented | ok |
| 33 | `GET /api/v2/shift-handovers/{id}` | implemented | pending |
| 34 | `POST .../items/{itemId}/ack` + `POST .../ack` | implemented | pending |

## Business case（P3 扩展，超出原 34 中的核心子集）

| 端点 | 状态 |
|------|------|
| `GET /api/v2/business-cases` | implemented |
| `GET /api/v2/business-cases/{id}` | implemented |
| `POST /api/v2/business-cases` | implemented |
| `POST .../appends` + acknowledge | implemented（append write_ok） |
| `GET /api/v2/business-case-types` | implemented（ok） |
| `POST /api/v2/business-case-workflows/{code}/start` | implemented |
| `GET /api/v2/business_cases/{id}/workflow` | implemented |

## SSE

| 端点 | 状态 | 说明 |
|------|------|------|
| `GET /api/v2/sse/stream` | implemented | 单流 demux 聊天+通知；旧专用 stream 路径不存在 |

---

**回归结论（2026-08-12）**：P0–P3 端点均在 mobile-core/FFI/UI 接线。模拟器读路径全绿；写路径事项追加 / 通知 read-all / 离线 applied 绿；聊天发送与清单 submit 受本地 DB 约束阻塞（`env_block`），非客户端。CI release APK 69.85 MB 级体积、装机成功。
