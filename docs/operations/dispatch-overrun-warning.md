# 派工预排冲突预警运维手册

## 功能语义

预排冲突预警是**非阻断**提示：当当前工单仍在执行、预计可能占用下一单的共享实际指派人员时，给调度员提前量。

| 行为 | 是否允许 |
| --- | --- |
| 按预计完成时间向未来排程 | 是 |
| 派工 / 发布 / 重排 | 是（预警不禁用任何按钮） |
| 因当前单未实际完成而阻断下一单 | 否 |
| 因预警改变人员空闲/忙碌状态 | 否 |

**人员进入空闲的唯一事实依据**仍是实际完成回报（`actual_end_time`）。预计完成时间（ETA）只是排程与冲突预测信号。

## 触发条件

同时满足才进入预警窗口：

1. 当前单状态为 `in_progress`，且 `actual_end_time IS NULL`
2. 下一单状态为 `pending` 或 `assigned`，且有 `planned_start_time`
3. 两单至少共享一名**实际指派**人员（个人单 `individual_user_id` 或班组活跃成员）
4. `now >= next.planned_start_time - effective_lead_minutes`

### 提前量配置

- 系统默认：`5` 分钟
- 有效范围：`0..60`（含端点）；`0` 表示到计划开始时刻才触发
- 优先级：单次工单覆盖值 > 生成规则快照/部门当前规则值 > 系统默认

已发布工单应保留生成时的规则快照；后续改规则不得静默改写已发布订单上的快照值。

### ETA

- 有 `estimated_completion_time`：展示 `predicted_conflict_minutes = max(0, eta - next.planned_start_time)`
- 无 ETA：标记 `eta_missing=true`，文案提示「未回报预计完成时间」，**不得伪造持续时间**

## 确认 vs 关闭

| 操作 | API | 含义 |
| --- | --- | --- |
| 确认（acknowledge） | `POST /api/v2/dispatch/alerts/{id}/acknowledge` | 调度员已看到；告警仍保持未关闭 |
| 关闭（resolve） | `POST /api/v2/dispatch/alerts/{id}/resolve` | 人工关闭告警 |
| 自动关闭 | 系统 | 实际完成、下一单取消、人员调整或冲突消失 |

同一「当前单 → 下一单」使用幂等键：

```text
dispatch_schedule_overrun:{current_order_id}:{next_order_id}
```

持续冲突只保留一条可更新告警。冲突关闭后再次出现时复用键、递增 `occurrence_count`、清空确认并再通知一次。

## 检测架构

1. **领域事件即时评估**：航班状态/资源/航段相关事件触发受影响航班工单链评估
2. **订单生命周期即时评估**：开始、ETA 回报、完成、取消、改派后 best-effort 评估
3. **30 秒定时扫描兜底**：覆盖事件遗漏；单进程注册一次，不在 HTTP 请求中 `spawn`

SSE：主题 `dispatch_alerts`，事件名 `dispatch_overrun_warning`；前端按 `dedupe_key` 幂等更新。

## 环境变量

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `DISPATCH_OVERRUN_WARNING_ENABLED` | `true` | 总开关；关闭后评估/扫描不写入告警 |
| `DISPATCH_OVERRUN_SSE_ENABLED` | `true` | 是否广播 SSE |
| `DISPATCH_OVERRUN_SCAN_INTERVAL_SECS` | `30` | 扫描间隔（秒） |

生产建议：分阶段开启（先持久化观察，再开 SSE/UI 通知）。

## HTTP API

| 方法 | 路径 | 权限 |
| --- | --- | --- |
| GET | `/api/v2/dispatch/alerts?unresolved=true&flight_id=` | `dispatch:view` |
| POST | `/api/v2/dispatch/alerts/{id}/acknowledge` | `dispatch:manage` |
| POST | `/api/v2/dispatch/alerts/{id}/resolve` | `dispatch:manage` |

## 健康检查与指标

关注：

- 扫描耗时、每次扫描评估对数
- 活跃未关闭告警数
- 自动关闭次数、ETA 缺失次数
- 事件处理失败次数

结构化日志应包含 `flight_id`、`current_order_id`、`next_order_id`、`dedupe_key`。

## Staging 冒烟

1. 创建两张共享同一人员的相邻工单，下一单计划开始在 5 分钟内
2. 开始当前单且不回报完成
3. 确认出现**一条**预警；重复扫描不产生重复告警
4. 确认（acknowledge）后告警仍在列表，标记已确认
5. 回报当前单实际完成后告警自动关闭
6. 全程确认派工、发布、重排按钮可用

## 回滚顺序

1. 将 `DISPATCH_OVERRUN_WARNING_ENABLED=false`（立即停止新写入与扫描副作用）
2. 前端隐藏预警条（或关闭 UI flag）
3. 如需回退 schema：保留 migration 字段（可空），勿在生产 drop 列；仅停用服务即可
4. 历史 `dispatch_alerts` 中 `alert_type=dispatch_schedule_overrun` 可保留审计
