# ADR-0003 Domain Event Outbox CDC Relay

- 状态：Accepted
- 日期：2026-03-27
- 事实源对齐刷新：2026-07-11 — 缺口修复（INSERT 收敛、stream_key→topic 重命名、outbox 启用可见性）；路径与 owner 以 Rust `services/api-server` 为准

## 决策

`domain_event_outbox` 的主链路发布方式从 SQL 轮询切换为 PostgreSQL logical replication CDC。

固定链路为：

```
命令输入 → 聚合状态变更 → 仓储原子保存
         →（同事务）domain_event_outbox
         → CDC relay → MessageQueue topic（默认 fms.domain-events）
         → subscriber / process managers → 下游处理
```

与 [ADR-0002](ADR-0002-flight-core-write-boundary.md) 衔接：写侧只落库 + outbox，不直接广播实时通道、缓存或前端形状。

### 约束

1. **写侧只负责**把事件写入 `domain_event_outbox`（与业务写同事务），**不**在写路径直接发布 MQ / SSE / 缓存失效广播。
2. **CDC relay 是主链路**；SQL 扫描只保留为失败补偿和滞留恢复（`recover_once`），不再承担热点发布职责。
3. **内部事件总线**经 `MessageQueue` 端口投递（当前装配为 RocketMQ gateway；默认 topic `EVENTS_DOMAIN_TOPIC` = `fms.domain-events`）。subscriber 与各 bounded process manager 的消费契约不变：消息体固定字段见下。
4. **同一环境内只允许一个 relay owner**（一个 CDC slot / publication 消费者）。**当前默认 owner 为 Rust API server 的后台 job 角色**（`runtime_role` 启用 background jobs 时），不是 Python worker。

### 发布消息体（固定字段）

CDC 解码与 SQL recovery 共用 `DomainEventOutboxDelivery::publish_row`，body 字段：

- `event_id`
- `aggregate_type`
- `aggregate_id`
- `event_type`
- `occurred_at`
- `payload`
- `source_change_id`

## 当前事实源（Rust）

| 职责 | 路径 |
|------|------|
| Outbox 行模型 | `services/api-server/crates/domain/src/events/mod.rs`（`DomainEventOutboxRow`） |
| pgoutput 解码 | `services/api-server/crates/domain/src/pgoutput_decoder.rs`（`PgOutputDecoder`；仅消费 INSERT） |
| CDC 主链路 | `services/api-server/crates/application/src/services/domain_event_cdc_relay_service.rs`（`DomainEventCdcRelayService`） |
| SQL 补偿 / 滞留恢复 | `services/api-server/crates/application/src/services/domain_event_relay_service.rs`（`DomainEventRelayService::recover_once`） |
| 投递与回写状态 | `services/api-server/crates/application/src/services/domain_event_outbox_delivery.rs`（`DomainEventOutboxDelivery`：publish、mark_published、mark_failed、backoff） |
| 下游订阅编排 | `services/api-server/crates/application/src/services/domain_event_subscriber_service.rs` |
| Outbox 仓储（INSERT + 发布状态） | `services/api-server/crates/infrastructure/src/repositories/pg_domain_event_outbox_repository.rs`（`insert_event` 关联函数 / `insert_event_auto` / `mark_published_batch` / `mark_failed`） |
| 组合根装配 | `services/api-server/crates/server/src/di/observability.rs`（构造 CDC + SQL relay + subscriber；`background_jobs_enabled` 时 `cdc.start()` + `scheduler.start()`） |
| 调度：SQL recovery 周期任务 | `services/api-server/crates/api/src/services/scheduler_runtime_service.rs`（任务名 `domain_event_outbox_retry_recovery`） |
| 进程生命周期 | `services/api-server/crates/server/src/main.rs`（shutdown 时 `cdc_relay_svc.stop()`） |
| Topic / 开关默认 | `EVENTS_DOMAIN_TOPIC`、`EVENTS_OUTBOX_ENABLED`、`EVENTS_OUTBOX_CDC_PUBLICATION_NAME`（默认 `fms_domain_event_outbox_pub`）、`EVENTS_OUTBOX_CDC_SLOT_NAME`（默认 `fms_domain_event_outbox_slot`）、replication 连接 `DB_REPLICATION_*` |

### Owner 与运行形态

| 角色 | 组件 | 说明 |
|------|------|------|
| **主链路 owner（当前）** | Rust `DomainEventCdcRelayService` | 在 DI 中于 background jobs 角色启动；logical replication + pgoutput；只处理 `public.domain_event_outbox` 的 **INSERT** |
| **补偿 owner（当前）** | Rust `DomainEventRelayService` + `SchedulerRuntimeService` | 周期性 SQL 拉取未发布/可重试行；**不是**热点路径 |
| **历史 owner（已退役）** | Python `worker` / `domain_event_relay_service` | 见下方归档；**不得**再作为默认 relay owner 描述 |

同一环境仍须保证 **单一 CDC slot 消费者**（多实例 background 角色争用 slot 会失败或重复配置风险）；API-only 角色跳过 CDC/scheduler 启动。

## 识别出的核心事实

- outbox **INSERT** 由各写路径在业务事务内经 `PgDomainEventOutboxRepository::insert_event`（关联函数，接受 `&mut Transaction`）或 `insert_event_auto`（自动提交）完成；`PgDomainEventOutboxRepository` 覆盖 INSERT + 发布回写，写侧扩展时须保持与 CDC 解码列一致。
- CDC relay **只消费** `domain_event_outbox` 的 `INSERT`（publication 创建时 `publish = 'insert'`），不消费 `published_at` / `last_error` 等 UPDATE，避免回环。
- 发布成功后回写 `published_at` / `publish_attempts`；失败更新 `next_retry_at` / `last_error` / `publish_attempts`（经 `DomainEventOutboxDelivery` + outbox repo）。
- 下游幂等继续依赖 `event_id` / `source_change_id` 与 subscriber 侧去重语义。
- 总线实现是 **MessageQueue 端口**（装配侧 RocketMQ），方法名统一为 `topic()`；**不再以 Redis Stream 为当前事实实现**（历史表述见归档）。

## 已知缺口（诚实记录）

- ~~Outbox **写入**未完全收敛到单一 port/repository；多处直接 `INSERT INTO domain_event_outbox`~~ — **已解决（2026-07-11）**：`PgDomainEventOutboxRepository` 新增 `insert_event` / `insert_event_auto`，4 处写路径（`OutboxBusinessCaseEventPublisher`、`flight_service`、`timeline`、`domain_action_executor`）全部改用 repository。
- ~~文档与代码中 `stream_key` 命名来自历史 Redis Stream 模型~~ — **已解决（2026-07-11）**：5 个服务的 `stream_key()` 方法重命名为 `topic()`；DB 列 `domain_event_consumer_offsets.stream_key` 经 migration 105 重命名为 `topic`；JSON payload 字段同步更新。
- 若 `EVENTS_OUTBOX_ENABLED=false` 或 background jobs 未启动，主链路与补偿均不跑——部署须显式启用 worker/background 角色。**已增强可见性（2026-07-11）**：`observability.rs` 在 `background_jobs_enabled` 与 `EVENTS_OUTBOX_ENABLED` 配置不一致时发出 `tracing::warn`。

## 迁移与运维要求

- PostgreSQL 主库必须启用 logical replication；CDC 服务会确保 publication / replication slot（名称可配置）。
- Rust 进程生命周期必须显式 `start`/`stop` CDC relay（DI + `main` shutdown），**不能**把主链路发布重新塞回 scheduler 轮询。
- 新增 outbox 列或事件形状时，须兼容 CDC 解码与上述固定消息字段。
- 环境内只跑一个 CDC owner 实例（或等价单 slot 消费者策略）。

## 历史（Python / Redis Stream，仅归档）

以下**不再**作为当前事实源：

- ~~默认 relay owner = Python `worker`~~
- ~~主链路 SQL 轮询发布~~
- ~~内部总线 = Redis Stream 作为唯一实现描述~~（代码已迁到 `MessageQueue` / RocketMQ topic；subscriber 契约字段仍兼容）
- ~~`src/application/services/domain_event_relay_service.py`（Python worker）~~

## 相关文档

- [ADR-0002 航班核心写链路边界](ADR-0002-flight-core-write-boundary.md)（写侧只落库 + outbox）
- [ADR-0001 路由-服务边界](ADR-0001-route-service-boundary.md)
- [SOURCE_OF_TRUTH](../SOURCE_OF_TRUTH.md)
