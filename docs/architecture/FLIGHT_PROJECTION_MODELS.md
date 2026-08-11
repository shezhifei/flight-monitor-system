# Flight Projection Models

## 标准投影视图

- 详情投影：`build_flight_dict_for_cache(flight)` 产出的标准字典
- 列表投影：与详情投影同字段契约，按分页缓存为 `list[dict]`
- 实时广播投影：基于详情投影字典发出字段变更和状态变更通知

## 事件到视图映射

- `flight.created_v2` → 详情缓存刷新 + 列表版本失效 + 列表追加 + 更新广播
- `flight.resource_updated_v2` → 详情缓存刷新 + 列表版本失效 + 更新广播
- `flight.leg_upserted_v2` → 详情缓存刷新 + 列表版本失效 + 更新广播
- `flight.remarks_updated_v2` → 详情缓存刷新 + 列表版本失效 + 更新广播
- `flight.status_updated_v2` → 上述全部 + 状态广播 + PubSub 状态广播

## 回放能力

- `FlightProjectionService.replay_events(...)` 支持按事件序列重建当前缓存与实时投影
- `FlightProjectionService.rebuild_projection(...)` 支持针对单个航班按当前事实状态重建详情/列表/实时视图

