"""
AIP (Artificial Intelligence Platform) 模块

提供基于 Ontology 的 AI 能力扩展，参考 Palantir AIP 模式设计。

核心组件：
- Ontology Layer: 业务对象建模
- Function Registry: 统一函数注册
- AIP Pipeline: LLM 编排管道
"""

from .app import AIPApplication

__all__ = [
    "AIPApplication",
]
