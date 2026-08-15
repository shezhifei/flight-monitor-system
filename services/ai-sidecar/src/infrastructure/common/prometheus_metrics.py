"""
P0-5-C: Prometheus metrics for AI Sidecar outbox monitoring
"""

from prometheus_client import Counter, Gauge, Histogram, REGISTRY
from typing import Optional


# AI Sidecar Outbox Queue Depth Gauge
# Tracks the number of pending messages in the MQ outbox queue
ai_sidecar_outbox_queue_depth = Gauge(
    'ai_sidecar_outbox_queue_depth',
    'Current depth of pending messages in the AI Sidecar MQ outbox queue',
    labelnames=['queue_name'],
    namespace='ai_sidecar'
)

# AI Sidecar MQ Publish Success Counter
ai_sidecar_mq_publish_success = Counter(
    'ai_sidecar_mq_publish_success_total',
    'Total number of successful MQ publishes by the AI Sidecar',
    labelnames=['event_type'],
    namespace='ai_sidecar'
)

# AI Sidecar MQ Publish Failure Counter
ai_sidecar_mq_publish_failure = Counter(
    'ai_sidecar_mq_publish_failure_total',
    'Total number of failed MQ publishes by the AI Sidecar',
    labelnames=['event_type', 'error_category'],
    namespace='ai_sidecar'
)

# AI Sidecar Tool Execution Duration Histogram
ai_sidecar_tool_execution_duration = Histogram(
    'ai_sidecar_tool_execution_duration_seconds',
    'Time spent executing tools in the AI Sidecar',
    labelnames=['tool_name'],
    namespace='ai_sidecar',
    buckets=(0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0)
)

# AI Sidecar Agent Loop Rounds Histogram
ai_sidecar_agent_loop_rounds = Histogram(
    'ai_sidecar_agent_loop_rounds',
    'Number of tool rounds in each agent loop execution',
    namespace='ai_sidecar',
    buckets=(1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 25, 30, 40, 50)
)


class AIPrometheusMetrics:
    """Context manager for AI Sidecar Prometheus metrics."""
    
    def __init__(self):
        self.registry = REGISTRY
    
    @staticmethod
    def set_outbox_queue_depth(queue_name: str, depth: int):
        """Set the current outbox queue depth."""
        ai_sidecar_outbox_queue_depth.labels(queue_name=queue_name).set(depth)
    
    @staticmethod
    def record_mq_publish_success(event_type: str):
        """Record a successful MQ publish."""
        ai_sidecar_mq_publish_success.labels(event_type=event_type).inc()
    
    @staticmethod
    def record_mq_publish_failure(event_type: str, error_category: str):
        """Record a failed MQ publish."""
        ai_sidecar_mq_publish_failure.labels(
            event_type=event_type,
            error_category=error_category
        ).inc()
    
    @staticmethod
    def record_tool_execution_duration(tool_name: str, duration_seconds: float):
        """Record tool execution duration."""
        ai_sidecar_tool_execution_duration.labels(tool_name=tool_name).observe(duration_seconds)
    
    @staticmethod
    def record_agent_loop_rounds(rounds: int):
        """Record number of rounds in an agent loop."""
        ai_sidecar_agent_loop_rounds.observe(rounds)
    
    def get_metrics_for_alerting(self) -> dict:
        """
        Get metrics suitable for alerting rules.
        
        Returns:
            Dict with metric names and thresholds for alerting:
            - queue_depth_threshold: 100 (alert if > 100 for 5 minutes)
            - failure_rate_threshold: 0.1 (alert if failure rate > 10%)
        """
        return {
            "queue_depth_threshold": 100,
            "failure_rate_threshold": 0.1,
            "queue_depth_metric": "ai_sidecar_outbox_queue_depth",
        }


# Global metrics instance
metrics = AIPrometheusMetrics()
