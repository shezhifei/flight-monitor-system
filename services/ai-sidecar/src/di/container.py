"""Dependency injection container.

Holds process-wide service state while layered installers perform wiring.
"""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    # 派工排班相关仓储
    # AI 相关服务与端口 (Task 14: 消除 Any | None)
    from src.application.interfaces.service_contracts import SSEHub
    from src.application.services.ai.todo_graph_pilot_ops_service import (
        TodoGraphPilotOpsService,
        TodoGraphPilotSnapshotService,
    )
    from src.application.services.anomaly.anomaly_query_service import AnomalyQueryService
    from src.application.services.anomaly.ports import (
        AnomalyFlightReadPort,
        AnomalyNotifyPort,
        AnomalyTodoWritePort,
    )
    from src.application.services.dispatch.dispatch_analytics_service import DispatchAnalyticsService
    from src.application.services.dispatch.dispatch_conflict_service import DispatchConflictService

    # 应用层服务
    from src.application.services.dispatch.dispatch_frontend_replan_service import DispatchFrontendReplanService
    from src.application.services.dispatch.dispatch_rule_service import DispatchRuleService
    from src.application.services.dispatch.dispatch_scenario_service import DispatchScenarioService
    from src.application.services.dispatch.dispatch_schedule_service import DispatchScheduleService
    from src.application.services.dispatch.flight_projection_service import FlightProjectionService
    from src.application.services.dispatch.personal_dispatch_optimizer import PersonalDispatchOptimizer
    from src.application.services.dispatch.qualification_coverage_service import QualificationCoverageService

    # 核心派工服务
    from src.application.services.dispatch.resource_availability_service import ResourceAvailabilityService
    from src.application.services.dispatch.rolling_horizon_optimizer import RollingHorizonOptimizer
    from src.application.services.flight.flight_command_gateway import FlightCommandGateway
    from src.application.services.flight.flight_import_service import FlightImportService
    from src.infrastructure.ai.config_store import AIConfigStoreInterface
    from src.infrastructure.ai.context_manager.manager import ContextManager
    from src.infrastructure.ai.conversation_manager.manager import ConversationManager
    from src.infrastructure.ai.tools.pending_actions.service import PostgresPendingActionStore
    from src.infrastructure.ai.tools.registry.service import ToolRegistry
    from src.infrastructure.repositories.postgresql.department_qualification_repository import (
        PostgreSQLDepartmentQualificationRepository,
    )
    from src.infrastructure.repositories.postgresql.department_task_type_requirement_repository import (
        PostgreSQLDepartmentTaskTypeRequirementRepository,
    )
    from src.infrastructure.repositories.postgresql.dispatch_travel_stats_repository import (
        PostgreSQLDispatchTravelStatsRepository,
    )
    from src.infrastructure.repositories.postgresql.flight_generation_rule_repository import (
        PostgreSQLFlightGenerationRuleRepository,
    )
    from src.infrastructure.repositories.postgresql.generation_adjustment_rule_repository import (
        PostgreSQLGenerationAdjustmentRuleRepository,
    )
    from src.infrastructure.repositories.postgresql.qualification_grant_repository import (
        PostgreSQLQualificationGrantRepository,
    )
    from src.infrastructure.repositories.postgresql.schedule_exception_repository import (
        PostgreSQLScheduleExceptionRepository,
    )
    from src.infrastructure.repositories.postgresql.shift_instance_repository import PostgreSQLShiftInstanceRepository
    from src.infrastructure.repositories.postgresql.shift_template_repository import PostgreSQLShiftTemplateRepository
    from src.infrastructure.repositories.postgresql.temporary_task_template_repository import (
        PostgreSQLTemporaryTaskTemplateRepository,
    )


class DIContainer:
    """依赖注入容器

    负责按正确顺序实例化和管理所有服务实例，确保单向依赖流。
    """

    def __init__(self):
        """初始化容器，定义所有服务的占位符"""
        self.cache_service: MemoryCache | None = None
        self.metrics_service: PerformanceMetricsService | None = None
        self.async_flight_repository: AsyncFlightRepositoryImpl | None = None
        self.async_connection_pool: AsyncPooledDatabaseConnection | None = None
        self.ai_query_connection_pool: AsyncPooledDatabaseConnection | None = None
        self.user_repository: PostgreSQLUserRepository | None = None
        self.role_repository: PostgreSQLRoleRepository | None = None
        self.permission_repository: PostgreSQLPermissionRepository | None = None
        self.async_todo_repository: AsyncTodoRepositoryImpl | None = None
        self.todo_agent_context_repository: TodoAgentContextRepository | None = None
        self.anomaly_repository: AnomalyRepositoryImpl | None = None
        self.kpi_view_repository: KPIViewRepository | None = None
        self.notification_repository: NotificationRepository | None = None
        self.business_case_workflow_run_repository: BusinessCaseWorkflowRunRepository | None = None
        self.flight_external_sync_repository: FlightExternalSyncRepository | None = None
        self.dispatch_collaboration_event_repository: DispatchCollaborationEventRepository | None = None
        self.dispatch_safety_checklist_repository: DispatchSafetyChecklistRepository | None = None
        self.shift_handover_repository: ShiftHandoverRepository | None = None
        self.operator_identity_context_repository: OperatorIdentityContextRepository | None = None
        self.todo_chain_template_repository: TodoChainTemplateRepository | None = None
        self.flight_ai_query_repository: FlightAIQueryRepository | None = None
        self.dispatch_chat_repository: DispatchChatRepository | None = None
        self.mobile_device_repository: MobileDeviceRepository | None = None
        self.mobile_upload_repository: MobileUploadRepository | None = None
        self.async_flight_service: AsyncFlightApplicationService | None = None
        self.flight_query_service: FlightQueryService | None = None
        self.domain_event_relay_service: DomainEventRelayService | None = None
        self.domain_event_subscriber_service: DomainEventSubscriberService | None = None
        self.async_todo_service: AsyncTodoApplicationService | None = None
        self.todo_scheduler_service: TodoSchedulerService | None = None
        self.todo_chain_service: TodoChainService | None = None
        self.async_business_case_service: AsyncBusinessCaseService | None = None
        self.anomaly_detection_service: AnomalyDetectionService | None = None
        self.anomaly_auto_resolver: AnomalyAutoResolver | None = None
        self.anomaly_flight_read_port: AnomalyFlightReadPort | None = None
        self.anomaly_todo_write_port: AnomalyTodoWritePort | None = None
        self.anomaly_notify_port: AnomalyNotifyPort | None = None
        self.alert_escalation_service: AlertEscalationService | None = None
        self.kpi_aggregation_service: KPIAggregationService | None = None
        self.notification_service: NotificationService | None = None
        self.dispatch_collaboration_recorder: DispatchCollaborationRecorder | None = None
        self.dispatch_collaboration_query_service: DispatchCollaborationQueryService | None = None
        self.dispatch_safety_checklist_service: DispatchSafetyChecklistService | None = None
        self.shift_handover_service: ShiftHandoverService | None = None
        self.operator_identity_service: OperatorIdentityService | None = None
        self.dispatch_chat_service: DispatchChatService | None = None
        self.mobile_device_service: MobileDeviceService | None = None
        self.mobile_workbench_service: MobileWorkbenchService | None = None
        self.mobile_operations_service: MobileOperationsService | None = None
        self.mobile_upload_service: MobileUploadService | None = None
        self.nl_query_service: NLQueryService | None = None
        self.llm_eval_service: LLMEvalService | None = None
        self.auth_service: AuthService | None = None
        self.auth_admin_query_service: AuthAdminQueryService | None = None
        self.auth_admin_command_service: AuthAdminCommandService | None = None
        self.ai_config_store: AIConfigStoreInterface | None = None
        self.ai_context_manager: ContextManager | None = None
        self.ai_conversation_manager: ConversationManager | None = None
        self.ai_execution_repository: PostgresAgentExecutionRepository | None = None
        self.ai_pending_action_store: PostgresPendingActionStore | None = None
        self.ai_rate_limiter: RateLimiter | None = None
        self.ai_executor: TodoAgentExecutor | None = None
        self.todo_agent_service: TodoAgentService | None = None
        self.todo_graph_pilot_snapshot_service: TodoGraphPilotSnapshotService | None = None
        self.todo_graph_pilot_ops_service: TodoGraphPilotOpsService | None = None
        self.smart_monitor: SmartMonitor | None = None
        self.flowable_integration_client: FlowableIntegrationClient | None = None
        self.shenzhen_airport_official_flight_source: ShenzhenAirportOfficialFlightSource | None = None
        self.flightaware_aircraft_identity_source: FlightAwareAircraftIdentitySource | None = None
        self.flight_external_sync_service: FlightExternalSyncService | None = None
        self.flowable_application_service: FlowableApplicationService | None = None
        self.bpmn_workflow_rule_parser: BpmnWorkflowRuleParser | None = None
        self.business_case_workflow_orchestrator: BusinessCaseWorkflowOrchestrator | None = None
        self.receipt_driven_workflow_coordinator: ReceiptDrivenWorkflowCoordinator | None = None
        self.workflow_dispatch_task_auto_runner: WorkflowDispatchTaskAutoRunner | None = None

        # 派工系统仓储
        self.dispatch_department_repository: PostgreSQLDepartmentRepository | None = None
        self.dispatch_team_type_repository: PostgreSQLTeamTypeRepository | None = None
        self.dispatch_team_repository: PostgreSQLTeamRepository | None = None
        self.dispatch_team_member_repository: PostgreSQLTeamMemberRepository | None = None
        self.dispatch_equipment_type_repository: PostgreSQLEquipmentTypeRepository | None = None
        self.dispatch_equipment_repository: PostgreSQLEquipmentRepository | None = None
        self.dispatch_stand_repository: PostgreSQLStandRepository | None = None
        self.dispatch_task_type_repository: PostgreSQLTaskTypeRepository | None = None
        self.dispatch_order_repository: PostgreSQLDispatchOrderRepository | None = None
        self.dispatch_order_member_repository: PostgreSQLDispatchOrderMemberRepository | None = None
        self.dispatch_alert_repository: PostgreSQLDispatchAlertRepository | None = None
        self.dispatch_service: DispatchService | None = None
        self.dispatch_query_service: DispatchQueryApplicationService | None = None
        self.dispatch_command_service: DispatchCommandApplicationService | None = None
        self.dispatch_resource_command_service: DispatchResourceCommandApplicationService | None = None
        # 核心派工服务
        self.resource_availability_service: ResourceAvailabilityService | None = None
        self.qualification_coverage_service: QualificationCoverageService | None = None
        self.dispatch_rule_service: DispatchRuleService | None = None
        self.personal_dispatch_optimizer: PersonalDispatchOptimizer | None = None
        self.dispatch_rolling_optimizer: RollingHorizonOptimizer | None = None
        self.dispatch_schedule_service: DispatchScheduleService | None = None
        self.flight_projection_service: FlightProjectionService | None = None

        # 应用层服务
        self.dispatch_frontend_replan_service: DispatchFrontendReplanService | None = None
        self.dispatch_analytics_service: DispatchAnalyticsService | None = None
        self.dispatch_scenario_service: DispatchScenarioService | None = None
        self.flight_command_gateway: FlightCommandGateway | None = None
        self.flight_import_service: FlightImportService | None = None

        self.dispatch_calculator: DispatchCalculator | None = None

        # 派工排班相关仓储
        self.dispatch_shift_template_repository: PostgreSQLShiftTemplateRepository | None = None
        self.dispatch_shift_instance_repository: PostgreSQLShiftInstanceRepository | None = None
        self.dispatch_schedule_exception_repository: PostgreSQLScheduleExceptionRepository | None = None
        self.dispatch_department_qualification_repository: PostgreSQLDepartmentQualificationRepository | None = None
        self.dispatch_qualification_grant_repository: PostgreSQLQualificationGrantRepository | None = None
        self.dispatch_department_task_type_requirement_repository: (
            PostgreSQLDepartmentTaskTypeRequirementRepository | None
        ) = None
        self.dispatch_flight_generation_rule_repository: PostgreSQLFlightGenerationRuleRepository | None = None
        self.dispatch_generation_adjustment_rule_repository: PostgreSQLGenerationAdjustmentRuleRepository | None = None
        self.dispatch_temporary_task_template_repository: PostgreSQLTemporaryTaskTemplateRepository | None = None
        self.dispatch_travel_stats_repository: PostgreSQLDispatchTravelStatsRepository | None = None

        self.async_db_service: AsyncDatabaseService | None = None
        self.ai_query_db_service: AsyncDatabaseService | None = None
        self.permission_template_repository: PermissionTemplateRepository | None = None
        self.notification_port: SSENotificationAdapter | None = None
        self.sse_hub: SSEHub | None = None
        self.tool_registry: ToolRegistry | None = None

        # 在线状态服务
        self.online_history_repository: OnlineHistoryRepository | None = None
        self.online_status_cache: OnlineStatusCache | None = None
        self.online_status_service: OnlineStatusService | None = None
        self.alert_service: AlertService | None = None
        self.workflow_dispatch_service: WorkflowDispatchService | None = None
        self.dispatch_recommendation_service: DispatchRecommendationService | None = None
        self.history_service: HistoryService | None = None
        self.flight_insight_service: FlightInsightService | None = None
        self.process_document_parser: ProcessDocumentParser | None = None
        self.flowable_process_draft_service: FlowableProcessDraftService | None = None
        self.dispatch_conflict_service: DispatchConflictService | None = None
        self.anomaly_query_service: AnomalyQueryService | None = None

    def set_sse_hub(self, sse_hub_instance: SSEHub | None) -> None:
        """????????? SSE Hub?????????????"""
        self.sse_hub = sse_hub_instance

        if self.metrics_service:
            self.metrics_service.set_sse_hub(sse_hub_instance)

        if self.smart_monitor:
            self.smart_monitor.set_services(sse_hub=sse_hub_instance)
