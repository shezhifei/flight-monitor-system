# Flight Projection Models

## 标准投影视图

- **监控宽表**：`flight_monitor_rows` 是热列表真相。一格一行，进出港是列。`GET /api/v2/flights` 列表/搜索/计数只打这张表，禁止 JOIN `flights` / `flight_legs` 或把两班 Flight zip 成一行。
- **详情投影**：按 `flight_id` 读一班方向航班（`FlightService::get_flight`）。详情不是热列表。
- **列表响应缓存**：在宽表之上的加速；挂了必须还能单表扫库。`row_id` 不因建链/拆链改变。
- **实时广播**：subscriber 消费 `flight.*_v2` 后失效缓存并推 SSE；不在写路径内拼前端载荷当持久真相。

## 事件到视图映射

- `flight.created_v2` → 同 UoW 已写宽表；详情缓存刷新 + 列表缓存失效 + 更新广播
- `flight.resource_updated_v2` → 占用回写后同步宽表展示列；详情缓存刷新 + 列表缓存失效 + 更新广播
- `flight.leg_upserted_v2` → 方向航班字段变更后同步宽表对应 inbound_* / outbound_* 列
- `flight.remarks_updated_v2` → 详情缓存刷新 + 列表版本失效 + 更新广播
- `flight.status_updated_v2` → 上述全部 + 状态广播 + PubSub 状态广播

## 回放 / 重建

- 写路径漏投影会导致宽表与 `flights` 分叉。重建走 `FlightMonitorRowService` / 仓储 upsert，不靠查询时 concat。
- 历史 Python `FlightProjectionService.replay_events` 已退役；现行消费在 `domain_event_subscriber_service.rs`。

