# Mobile 34 端点回归清单

> 对照 `../plans/android-flutter-rust-rebuild-plan.md (local)` §0.5 与旧 App 对拍面。  
> 状态：`implemented` = mobile-core + FFI + UI 已接线；`core-only` = 仅 core/FFI；`n/a` = 后端已迁路径。  
> 验证列：模拟器对拍 200 记 `ok`；写路径 `write_ok`；环境阻塞 `env_block`；未跑 `pending`。  
> **收口日 2026-08-12**：读路径 smoke + 写路径 smoke + 离线 applied 已跑。  
> **P2 补齐（同日）**：Web/Native 双端 SSE ≤2s；飞行模式 30s 重连无重复 seq；聊天 send/read、清单 submit、通知 read/ack/回执组、交接班签收均 write_ok。

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
| 21 | `POST .../safety-checklist/items/{code}` | implemented | write_ok（本机需 `scripts/mobile/apply_local_write_paths.ps1`：record_id varchar(36)） |

## Chat（路径修正）

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 22 | `GET /api/v2/dispatch/collaboration/groups` | implemented | ok |
| 23 | `GET .../groups/{id}/messages` | implemented | ok |
| 24 | `POST .../groups/{id}/messages` | implemented | write_ok |
| 25 | `POST .../groups/{id}/read` | implemented | write_ok |

> 旧路径 `/api/v2/dispatch-chat/*` 已 404，勿再使用。

## Notifications

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 26 | `GET /api/v2/notifications` | implemented | ok |
| 27 | `GET /api/v2/notifications/unread-count` | implemented | ok |
| 28 | `POST /api/v2/notifications/{id}/read` | implemented | write_ok |
| 29 | `POST /api/v2/notifications/read-all` | implemented | write_ok |
| 30 | `POST /api/v2/notifications/{id}/ack` | implemented | write_ok（客户端 `ack`→`acknowledged`） |
| 31 | `GET /api/v2/notifications/receipt-groups/{id}` | implemented | write_ok |

## Shift handover

| # | 端点 | 状态 | 验证 |
|---|------|------|------|
| 32 | `GET /api/v2/shift-handovers` | implemented | ok |
| 33 | `GET /api/v2/shift-handovers/{id}` | implemented | write_ok |
| 34 | `POST .../items/{itemId}/ack` + `POST .../ack` | implemented | write_ok |

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
| `GET /api/v2/sse/stream` | implemented | 单流 demux；双端 ≤2s（chat 139–158ms / notif 24–121ms）；飞行 30s 后重连 `connected=2`、seq 去重 |

---

**回归结论（2026-08-12）**：P0–P3 端点均在 mobile-core/FFI/UI 接线。模拟器读路径全绿；写路径聊天 send/read、清单 submit、通知 read/ack/回执组、交接班签收、事项追加、离线 applied 绿。清单/通知写路径依赖本机 schema 补丁（UUID record_id / `notifications.updated_at`），脚本 `scripts/mobile/apply_local_write_paths.ps1`，**不是客户端 bug**。P2 双端与 SSE 重连专项已过。CI release APK 69.85 MB 级体积、装机成功。
