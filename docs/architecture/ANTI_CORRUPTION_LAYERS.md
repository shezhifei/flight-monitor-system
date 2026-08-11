# Anti-Corruption Layers

反腐层 / 端口：领域与外部系统（DB、MQ、Flowable、缓存、AI 引擎）之间的稳定契约。
依赖方向见 [DEPENDENCY_DIRECTION](DEPENDENCY_DIRECTION.md)；组合根在 `services/api-server/crates/server/`。

## 当前事实源（Rust `services/api-server`）

端口定义：`services/api-server/crates/domain/src/ports/`
适配器实现：`services/api-server/crates/infrastructure/src/`（`repositories/`、`messaging/`、`integrations/`、`cache/`、`security/` 等）

### 核心与外部边界端口

| 职责 | 端口（domain） | 典型适配器（infrastructure） |
|------|----------------|------------------------------|
| 航班写/读仓储 | `ports/flight_repository.rs` | `repositories/pg_flight_repository.rs`；装饰 `cached_flight_repository.rs` |
| 航班缓存后端 | `ports/flight_cache_backend.rs` | `cache/flight_cache_backend.rs` |
| 航班运行时投影 | `ports/flight_runtime_projection_repository.rs` | `repositories/pg_flight_runtime_projection_repository.rs` |
| 航班归档 / 外部同步 | `ports/flight_archive_repository.rs`、`flight_sync_repository.rs` | `pg_flight_archive_repository.rs`、`pg_flight_sync_repository.rs` |
| 消息队列 | `ports/message_queue.rs` | `messaging/`（`MessageQueueGatewayClient`、`RocketMqPushConsumer`、`MemoryPushConsumer`） |
| Flowable 网关 | `ports/flowable_gateway.rs` | `integrations/flowable_client.rs` |
| 运行时诊断 | `ports/runtime_diagnostic_repository.rs`、`runtime_diagnostic_sink.rs` | `repositories/pg_runtime_diagnostic_event_repository.rs` |
| 防重放 nonce | `ports/nonce_replay_store.rs` | `security/anti_replay_store.rs` |
| 会话运行时 | `ports/session_runtime_repository.rs` | `repositories/session_runtime_repository.rs` |

### 其他已建立的 domain 端口（仓储类）

`anomaly_repository`、`business_case_repository`、`business_case_workflow_run_repository`、`dispatch_repository`、`dispatch_collaboration_repository`、`event_rule_repository`、`label_repository`、`mobile_repository`、`notification_repository`、`online_history_repository`、`operator_identity_repository`、`permission_template_repository`、`shift_handover_repository`、`todo_repository`、`todo_agent_context_repository`、`user_repository`、`workflow_dispatch_repository`、`workflow_form_repository`，以及 AI 控制面：`ai_*_repository` / `ai_auth_context_loader`。

实现多在 `infrastructure/src/repositories/pg_*.rs`；装配见 `server/src/di/`。

## AI sidecar（`services/ai-sidecar`，仍有效）

Python 仅承接 AI 侧车；分层镜像 Rust，端口在：

- 领域端口：`services/ai-sidecar/src/domain/ports/`
  - `notification_port.py` — 通知抽象
  - `agent_runtime_port.py` — Agent 引擎边界
  - `service_interfaces.py` — 对主链业务服务的 Protocol（航班/待办/业务事项）
- 应用侧 AI 门面：`services/ai-sidecar/src/application/ports/ai_ports.py`（再导出 LLM/工具/配置等 infra 类型）
- DI：`services/ai-sidecar/src/di/container.py`；运行时 provider：`infrastructure/runtime/providers.py`

AI 不得绕过 Rust 写服务直接改航班核心表（见 [ADR-0002](ADR-0002-flight-core-write-boundary.md)）。

## 历史（Python 主链，仅归档）

以下路径**不再**作为当前 ACL 事实源（曾位于旧 monorepo `src/` 或 `legacy-backend/`）：

- ~~`src/domain/ports/monitoring_ports.py` + `src/application/runtime/monitoring_adapters.py`~~
- ~~`src/infrastructure/runtime/providers.py`~~（主链；sidecar 仍有自身 provider）
- ~~`src/infrastructure/repositories/serializers.py`~~
- ~~`src/application/services/flight/flight_write_effects.py`~~
- ~~`src/infrastructure/ai/tools/business_case_inputs.py`~~ / ~~`feature_flags.py`~~（主链路径）
- ~~`src/infrastructure/events/business_case_event_publisher.py`~~
- ~~`src/application/subscribers/business_case_event_subscriber.py`~~

## 本阶段替换原则（仍适用）

- 仓储 / 适配器不 import 应用层 mapper、SSE hub、路由交付模块
- 基础设施错误与告警经 port，不直连应用层 SSE / 告警服务
- AI 工具依赖端口或 sidecar 内契约，不依赖主链应用层 DTO / feature flag 模块
- 新增边界优先在 `domain/src/ports/` 声明 trait，再由 infrastructure 实现

## 相关文档

- [DEPENDENCY_DIRECTION](DEPENDENCY_DIRECTION.md)
- [ADR-0002 航班核心写链路](ADR-0002-flight-core-write-boundary.md)
- [SOURCE_OF_TRUTH](../SOURCE_OF_TRUTH.md)
