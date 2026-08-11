# Flight Core File Whitelist

> ⚠️ 历史文档：本白名单描述旧 Python 主链的航班核心文件。当前事实源已切换为 Rust
> `services/api-server`（见 [ADR-0002 航班核心写链路](ADR-0002-flight-core-write-boundary.md)）。
> 下列 Python 路径不再作为当前事实源；Rust 对应职责现分布于 `domain/src`、`application/src/services/flight_service.rs`、
> `application/src/services/flight_commands.rs`、`application/src/services/flight_runtime_service/` 与下方标记的文件。

以下文件属于航班核心主链（历史 Python 路径，已归档）：

- ~~`src/domain/models/flight.py`~~ → Rust 领域模型：`services/api-server/crates/domain/src/models/flight.rs`
- ~~`src/domain/models/state_changes.py`~~（历史 Python 主链，Rust 无 1:1 文件；状态变迁内聚于 `domain` 模型与 `flight_service.rs`）
- ~~`src/domain/aggregates/flight.py`~~（历史 Python 主链，Rust 无 1:1 聚合文件）
- ~~`src/application/services/flight/flight_command_context.py`~~（历史 Python 主链，Rust 无 1:1 文件）
- ~~`src/application/services/flight/flight_update_translator.py`~~（历史 Python 主链，Rust 无 1:1 文件）
- ~~`src/application/services/flight/flight_write_commands.py`~~ → Rust：`services/api-server/crates/application/src/services/flight_commands.rs`
- ~~`src/application/services/flight/flight_write_policy.py`~~（历史 Python 主链，Rust 无 1:1 文件）
- ~~`src/application/services/flight/async_flight_service.py`~~ → Rust：`services/api-server/crates/application/src/services/flight_service.rs`
- ~~`src/infrastructure/repositories/async_flight_repository_impl.py`~~（历史 Python 主链，Rust 仓储实现于 `infrastructure/src/repositories/pg_*.rs`）
- ~~`src/infrastructure/repositories/serializers.py`~~（历史 Python 主链，Rust 无 1:1 文件）
- ~~`src/application/services/domain_event_relay_service.py`~~ → Rust：`services/api-server/crates/application/src/services/domain_event_relay_service.rs`
- ~~`src/application/services/domain_event_subscriber_service.py`~~ → Rust：`services/api-server/crates/application/src/services/domain_event_subscriber_service.rs`

## 白名单约束

- 允许：命令翻译、聚合状态变更、原子保存、事件发出、写计划定义
- 禁止：页面协议适配、缓存形状兼容、前端输出拼装、直接推流、与具体 UI 消费方式绑定

## 非白名单文件的限制

- 非白名单文件可以触发命令，但不得定义航班状态真相
- 非白名单文件不得直接写入 Flight 仓储并绕过写计划/聚合主链

