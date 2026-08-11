# Runtime Context Factory

## 目标

- 默认运行时装配由显式工厂负责，而不是由组合根模块全局单例隐式创建

## 当前实现

- `src/application/runtime/runtime_factory.py`
  - `create_default_application_container()`
  - `create_default_lifecycle_manager()`
  - `create_default_runtime()`

## 当前约束

- `app_factory` 通过显式工厂创建默认容器与生命周期管理器
- `composition_root.py` 仅保留兼容用途，不再作为运行时装配事实来源
- 应用层禁止再直接导入 `get_session_manager()` / `get_performance_metrics()` / `composition_root`

