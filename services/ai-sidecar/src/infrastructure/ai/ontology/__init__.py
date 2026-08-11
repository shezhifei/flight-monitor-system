"""
Ontology 模块 - 业务对象建模层

基于 Palantir AIP 模式，将业务领域建模为对象、属性和关系。
"""

from .schema import OntologyRegistry, OntologySchema, get_ontology_registry

__all__ = [
    "OntologyRegistry",
    "OntologySchema",
    "get_ontology_registry",
]
