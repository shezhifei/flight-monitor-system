"""验证 DIContainer 不包含 dead 属性。"""
import inspect
from src.di.container import DIContainer


DEAD_ATTRS = [
    "business_case_type_repository",
    "session_manager",
    "domain_event_cdc_relay_service",
    "business_case_event_publisher",
    "flowable_case_trigger",
    "business_case_workflow_starter",
    "dispatch_publication_service",
    "mobile_push_gateway",
    "flight_archive_service",
    "health_service",
    "resource_utilization_service",
    "system_flags_service",
    "scheduler_runtime_service",
    "system_ops_service",
]


def test_di_container_has_no_dead_attrs():
    source = inspect.getsource(DIContainer.__init__)
    for attr in DEAD_ATTRS:
        assert attr not in source, (
            f"DIContainer.__init__ still contains dead attribute '{attr}'. "
        )
