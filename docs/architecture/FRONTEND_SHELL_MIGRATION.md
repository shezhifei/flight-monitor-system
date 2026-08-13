# Frontend Shell Migration

> 历史迁移动态。现行前端路径见 `docs/SOURCE_OF_TRUTH.md` §6：`frontend/vue-app/dist/` → `/frontend/<page>.html`。

## 北极星

- 单一页面壳层：Vue 3 + Vite MPA 架构，20 个独立入口
- 统一认证桥接：继续复用 `authBridge`
- 统一数据访问客户端：继续复用 `apiClient`
- 统一实时订阅客户端：继续复用 `EventSourceClient`

## 页面分层

### Vue 3 MPA 入口（已迁移）
- `login.html`
- `flight_monitor.html`
- `dispatch_board.html`
- `dashboard.html`
- `dispatch_rule_center.html`
- `resource_manager.html`
- `resource_utilization.html`
- `kpi_dashboard.html`
- `anomaly_monitor.html`
- `system_status.html`
- `system_flags.html`
- `user_manager.html`
- `operations_review_report.html`
- `flight_imports.html`
- `command_center.html`
- `dashboard_frontline_workbench.html`
- `dashboard_handover.html`
- `flowable_modeler.html`
- `ai_monitor.html`（Vue 壳层）

### 独立保留（React 子应用）
- `ai_config_center.html`
- `llm_eval_lab.html`
- `nl_query.html`

## 本阶段完成的收口

### Vue 3 + Vite + TypeScript 迁移（2026-04-03 完成）

- **20 个页面全部迁移**：从 `frontend/html/` 迁移到 `frontend/vue-app/src/pages/`
- **构建产物**：`frontend/dist/` 目录输出 20 个 HTML 文件
- **TypeScript 全面覆盖**：所有业务代码迁移为 `.ts` / `.vue` 文件
- **ECharts 优化**：从 ~10k 行内联代码迁移到 npm echarts + 共享 chunk
  - dispatch_board.js: 1,169 kB → 48 kB（96% 缩减）
- **Web Worker 迁移**：dispatch_replan_worker.js → dispatchReplanWorker.ts

### 历史文件备份

- `frontend/legacy-backup/html/` — 旧版 20 个 HTML 页面
- `frontend/legacy-backup/js/` — 旧版 ~32 个 JS 文件
- `frontend/legacy-backup/css/` — 旧版 16 个 CSS 文件

## 遗留页面停更规则

- 不再向 `frontend/legacy-backup/js/*.js` 超大历史文件引入新的业务入口
- 新增前端功能必须通过 `frontend/vue-app/src/` 接入
- AI React 子应用保持独立，仅通过 mount point 挂载

## 相关文档

- `docs/FRONTEND_MIGRATION.md` — 完整迁移文档
- `QUICK_START.md` — 构建与启动指南

