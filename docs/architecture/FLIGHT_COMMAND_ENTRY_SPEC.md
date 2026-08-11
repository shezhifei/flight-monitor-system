# Flight Command Entry Spec

对齐 [ADR-0002](ADR-0002-flight-core-write-boundary.md)。当前写入口以 Rust 为准。

## API 写入口

```
routes/flights/crud.rs
  → FlightCreateCommand / FlightUpdateCommand
  → FlightService::execute_create / execute_update
  → domain 规则 → FlightRepository → Pg（+ 可选 cached 装饰）
  → 同事务 domain_event_outbox
```

| 职责 | 路径 |
|------|------|
| HTTP 写 | `services/api-server/crates/api/src/routes/flights/crud.rs` |
| 显式命令 | `application/src/services/flight_commands.rs` |
| 应用写服务 | `application/src/services/flight_service.rs`（`execute_create` / `execute_update`） |
| 领域模型 / 状态 | `domain/src/models/flight.rs`、`flight_state.rs` |
| 仓储端口 | `domain/src/ports/flight_repository.rs` |
| 仓储实现 | `infrastructure/src/repositories/pg_flight_repository.rs`（`cached_flight_repository` 可选） |
| 时间线写 | `routes/flights/timeline.rs` → `FlightRuntimeService`（同事务 `flight.timeline_*_v2` outbox） |

## 路由职责（仅此）

1. 认证与权限（如 `flight:manage`、受控字段拒绝）
2. 映射 HTTP body → `FlightCreateCommand` / `FlightUpdateCommand`
3. 调用 `FlightService::execute_create` / `execute_update`（兼容方法 `create_flight` / `update_flight` 仍可用）
4. 可选审计记录（`FlightRuntimeService::record_*`）
5. 封装 HTTP 响应

路由 **禁止**：直连 `FlightRepository`、原始 SQL、写后 SSE/缓存广播旁路、字段级状态机/策略编排后直接持久化。
写后实时与缓存由 outbox → subscriber 消费（见 [FLIGHT_WRITE_SEQUENCE](FLIGHT_WRITE_SEQUENCE.md)）。
`timeline` 路由同样禁止旁路广播；由 `flight.timeline_upserted_v2` / `flight.timeline_deleted_v2` 驱动。

## 统一写生命周期（与 FlightService 对齐）

1. 反序列化 API payload（`FlightCreate` / `FlightUpdate`）
2. 服务内校验（`validate_create_payload` / `validate_update_payload`）
3. DTO → 领域：`from_create` 或 `update_patch_from_dto` → `Flight` / `FlightUpdatePatch`
4. 状态变更应可对照 `flight_state::can_transition`（禁止无校验旁路扩散）
5. 经 port 持久化：`save_in_tx` / `update_partial_in_tx`（HTTP create/update 在服务内开事务）
6. **同事务**写 `domain_event_outbox`（`flight.*_v2` 事件族，见 ADR-0003 / [FLIGHT_WRITE_SEQUENCE](FLIGHT_WRITE_SEQUENCE.md)）
7. 热列表 / 负缓存失效（`invalidate_hot_list` 等）— **仅失效**
8. 外部事务路径：`update_flight_in_tx` 供 AI 等调用方编排；action 级 outbox 由调用方写入

## 非 API 写入口

- **AI**：ontology 映射 `DomainActionExecutor.Flight.*`（如 `update_status`、`change_stand`、`add_note`）
- **系统任务 / 流程**：须落到同一 `FlightService` 或同一 `FlightRepository` 写端口

不允许在 AI sidecar、脚本或外围驱动中自行 `UPDATE flights` / 只写缓存当真相。

## 明确禁止

- 绕过 `FlightRepository` 直接 SQL 改航班核心表
- 以缓存、SSE payload、投影表为写真相
- 在 application 层新增与航班核心写无关的「顺手 SQL」绕过 port

## 历史（Python，仅归档）

以下 **不再** 作为事实源：`flight_command_gateway.py`、`AsyncFlightApplicationService`、`FlightAggregate`、`AsyncFlightRepositoryImpl` 等。语义见 ADR-0002「历史」节。

## 相关文档

- [ADR-0002](ADR-0002-flight-core-write-boundary.md)
- [FLIGHT_WRITE_SEQUENCE](FLIGHT_WRITE_SEQUENCE.md)
- [ADR-0001 路由-服务边界](ADR-0001-route-service-boundary.md)
