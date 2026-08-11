# Flight Event to View Mapping

## 写入端与投影端分工

- 写入端只负责：聚合变更、仓储保存、outbox 发出
- 投影端负责：详情缓存、列表缓存版本、SSE 更新、状态广播、PubSub 状态广播

## 当前实现位置（Rust `services/api-server`）

- ~~投影消费器：`src/application/services/flight/flight_projection_service.py`~~（历史 Python 主链；Rust 中由 `services/api-server/crates/application/src/services/domain_event_subscriber_service.rs` 统一承接投影/缓存失效/SSE）
- 订阅接线：`services/api-server/crates/application/src/services/domain_event_subscriber_service.rs`
- ~~运行时 event-driven 写后策略：`src/application/services/flight/flight_write_effects.py`~~（历史 Python 主链；Rust 中由 `services/api-server/crates/application/src/services/flight_service.rs` 的写计划/副作用机制承接）

