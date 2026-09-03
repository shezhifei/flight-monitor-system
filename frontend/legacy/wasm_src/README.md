# Dispatch Wasm Solver

本目录保留 Rust wasm 历史实现，仅作为退场中的源码参考；生产 preview / apply 主链已切到 `frontend/vendor/ortools/active-manifest.json` 驱动的 OR-Tools wasm。

当前协议版本：

- `model_version`: `dispatch_wasm_full_model_v1`
- `solver_version`: `dispatch_solver_ortools_wasm_full_model_v2`（生产）

当前浏览器端正式求解链路共同消费以下事实输入：

- `optimizable_orders`
- `fixed_anchor_orders`
- `employee_anchor_states`
- `equipment_anchor_states`
- `employee_free_windows`
- `equipment_free_windows`
- `resource_travel_edges`
- `turnaround_pairs`
- `objective_config`

当前正式 solver 输出的原生结果结构：

- `order_results`
- `personnel_slot_assignments`
- `equipment_slot_assignments`
- `continuity_decisions`
- `objective_breakdown`
- `solver_run_metadata`

Rust 历史实现曾覆盖的约束类别：

- 人员槽位与设备槽位独立赋值
- 资源自由窗与锚点可达性校验
- 资源维度 travel 校验
- 过站连续性硬约束 / 软惩罚
- 基线变更、缺口、迟到、travel、稀缺资质、负载偏差联合计分

## Status

- 生产求解器：`tools/ortools_wasm/bridge/dispatch_replan_solver.cc`
- 运行时加载：`frontend/vendor/ortools/active-manifest.json`
- 本目录：不再是生产运行时入口

## Build

在仓库根目录执行：

```bash
wasm-pack build frontend/wasm_src --target web --out-dir pkg
```

产物位于 `frontend/wasm_src/pkg/`。这些产物不再由生产 worker 直接加载：

- `dispatch_solver_wasm.js`
- `dispatch_solver_wasm_bg.wasm`
