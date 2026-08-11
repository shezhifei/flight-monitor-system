# ADR-0002: 航班核心写链路边界

## 状态

已接受（2026-03-07）；事实源对齐（2026-07-10）；**实现闭环（2026-07-11）** — 同事务 outbox + subscriber 驱动 SSE/缓存。

## 背景

航班核心状态被 API、调度、AI 动作、缓存/SSE、流程等多入口触达。若任一入口可直接改库、改缓存或拼装“状态真相”，则：

- 状态机与权限校验被旁路；
- 写后副作用（缓存、实时、下游）与事务边界分裂；
- 无法保证 `domain_event_outbox` 与核心表同事务一致。

历史 Python 写链路（`FlightCommandGateway` / `AsyncFlightApplicationService` / `FlightAggregate`）已退役为归档语义；当前默认 HTTP 与写模型在 **Rust**。

## 决策

航班核心状态的唯一可信写链路固定为：

```
命令输入 → 应用服务编排 → 领域模型/状态规则 → 仓储原子保存
         →（同事务）domain_event_outbox → CDC/relay 下游消费
```

与 [ADR-0003](ADR-0003-domain-event-outbox-cdc-relay.md) 衔接：写侧只落库 + outbox，不直接广播 Redis / SSE / 前端形状。

### 约束

1. **只有领域规则下的写模型可以修改航班核心状态**（状态、机位/登机口、关键时刻与航节等写字段）。路由、AI 工具、流程回调、缓存层不得直接 `UPDATE flights` / 拼装真相。
2. **外部入口只能提交命令（或等价 DTO）**：HTTP 经应用服务；AI 经 `DomainActionExecutor` 映射的 `Flight.*` 动作；系统任务同样落到同一写服务/仓储端口。
3. **缓存、SSE、投影、AI、Flowable、异常、前端均为外围消费方**，不得回灌写链路或在写路径内硬编码前端协议。
4. **基础设施不得依赖应用层交付模块**（路由、SSE hub、mapper）完成写后广播或 DTO 映射；写后效应通过 outbox 消费或明确 port 适配。
5. 与 [ADR-0001](ADR-0001-route-service-boundary.md) 一致：`crates/api/src/routes/flights/**` **禁止** 直连 repository / 原始 SQL。

### 当前事实源（Rust）

| 职责 | 路径 |
|------|------|
| HTTP 写入口 | `services/api-server/crates/api/src/routes/flights/crud.rs` |
| 应用写服务 | `services/api-server/crates/application/src/services/flight_service.rs`（`create_flight` / `update_flight` / `update_flight_in_tx`） |
| 领域模型 | `services/api-server/crates/domain/src/models/flight.rs` |
| 状态变迁规则 | `services/api-server/crates/domain/src/models/flight_state.rs` |
| 仓储端口 | `services/api-server/crates/domain/src/ports/flight_repository.rs`（`FlightRepository` / `FlightUpdatePatch`） |
| 仓储实现 | `services/api-server/crates/infrastructure/src/repositories/pg_flight_repository.rs`；缓存装饰 `cached_flight_repository.rs` |
| 组合根装配 | `services/api-server/crates/server/src/di/` |
| Outbox 行模型 | `services/api-server/crates/domain/src/events/`（`DomainEventOutboxRow`） |
| Outbox 仓储 / 投递 | `pg_domain_event_outbox_repository`；`domain_event_*_service` / CDC relay（见 ADR-0003） |
| AI 域动作映射 | ontology / `DomainActionExecutor.Flight.*`（如 `update_status`、`change_stand`、`add_note`） |

### 允许的外围（非真相源）

- 热列表/读缓存失效：`FlightService::invalidate_hot_list`、cached repository 失效逻辑 — **仅失效，不定义状态**。
- 运行时投影：`flight_runtime_projection_repository` — 读模型，不得反向写核心表。
- SSE / 前端推送：订阅 outbox 或查询服务结果后推送，不在写事务内拼装前端 payload 作为持久真相。

## 明确禁止（不得继续加深）

- 路由层做字段策略、状态机、缓存形状兼容与协议编排后直接持久化。
- 基础设施 import 应用层路由 / SSE / application mapper 完成写后副作用。
- 写模型 API 暴露“前端字段名 / 缓存键 / 广播 channel”作为持久契约。
- AI sidecar 或脚本绕过 Rust 写服务，直接改 `flights` 或只写缓存当真相。
- 在 application 层新增与航班核心写无关的“顺手 SQL”绕过 `FlightRepository`（存量 SQL 债见 `application_boundary_inventory`，**航班写路径不得新增**）。

## 迁移与演进要求

1. **新增写能力**必须：route/gateway → `FlightService`（或后续显式 Command 类型）→ domain 规则 → `FlightRepository` → 同事务 outbox（若产生领域事件）。
2. **状态变更**应可对照 `flight_state::can_transition`；禁止无校验的 `update_status` 旁路扩散。
3. **AI / 流程写**：只允许 `Flight.*` 域动作或调用同一应用写服务；禁止 sidecar infra 直接拼 SQL 写航班核心。
4. **已落地的收敛**（2026-07-11）：
   - HTTP create/update 与 `update_flight_in_tx` 在保存航班的同一事务写入 `flight.*_v2` outbox；
   - `routes/flights/crud` 与 `timeline` 不再旁路 SSE/列表缓存失效；由 `DomainEventSubscriberService` 消费 outbox 后广播/失效；
   - status 变更校验 `flight_state::can_transition`；
   - 显式命令：`FlightCreateCommand` / `FlightUpdateCommand`（`flight_commands.rs`）；
   - 时间线写同事务 `flight.timeline_*_v2` outbox。
5. **可选后续**：领域层更重的聚合根（状态变更只经 `Flight` 方法）；其它域路由若仍旁路广播可按同样模式收敛。
6. 相关序列与入口说明：`FLIGHT_WRITE_SEQUENCE.md`、`FLIGHT_COMMAND_ENTRY_SPEC.md`。

## 影响

- 新功能优先扩 `FlightService` + domain/port，而不是加 route 内逻辑或新 SQL 捷径。
- 读优化（投影、缓存）可独立演进，但不得成为第二写源。
- 与 ADR-0001 守门测试一致：航班路由保持零「路由直连仓储」。

## 历史（Python，仅归档）

以下路径**不再**作为当前事实源：

- ~~`src/domain/models/flight.py` / `state_changes.py` / `aggregates/flight.py`~~
- ~~`src/infrastructure/repositories/async_flight_repository_impl.py`~~
- ~~`src/application/services/flight/flight_command_gateway.py`~~
- ~~`src/application/services/domain_event_relay_service.py`（Python worker 语义见 ADR-0003/0004 与当前 Rust relay 实现）~~

## 相关文档

- [ADR-0001 路由-服务边界](ADR-0001-route-service-boundary.md)
- [ADR-0003 Outbox CDC Relay](ADR-0003-domain-event-outbox-cdc-relay.md)
- [DEPENDENCY_DIRECTION](DEPENDENCY_DIRECTION.md)
- [TECH_DEBT_DASHBOARD](TECH_DEBT_DASHBOARD.md)
- [SOURCE_OF_TRUTH](../SOURCE_OF_TRUTH.md)
