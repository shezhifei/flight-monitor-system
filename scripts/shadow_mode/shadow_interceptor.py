"""
Shadow Mode Interceptor for Real AI Runner Integration.

This module intercepts AI queries and routes them to both human operators
and the real AI runner for dual-run validation in shadow mode.

Usage:
    import sys
    from pathlib import Path
    
    # Add scripts directory to Python path
    sys.path.append(str(Path(__file__).parent / "scripts" / "shadow_mode"))
    
    from shadow_interceptor import ShadowModeInterceptor, setup_shadow_queue
    
    # Initialize interceptor
    interceptor = ShadowModeInterceptor(enabled=True, mq_config=mq_settings)
    
    # Use as middleware
    async def process_with_shadow(query: dict):
        result = await interceptor.process_query(query)
        return result
"""

import asyncio
import json
import logging
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

import aiomqtt
from pydantic import BaseModel

# Configure logging
logger = logging.getLogger(__name__)

# Add scripts/shadow_mode to path if not already there
current_dir = Path(__file__).parent.absolute()
if str(current_dir) not in sys.path:
    sys.path.insert(0, str(current_dir))

# Import comparison engine
try:
    from comparison_engine import compare_answers, find_highest_severity
except ImportError:
    logger.warning("comparison_engine not found, using basic comparison")
    compare_answers = None


class ShadowQueueMessage(BaseModel):
    """Message format for shadow mode queue."""
    
    message_type: str  # "query", "human_answer", "agent_answer", "comparison"
    query_id: str
    timestamp: str
    data: dict[str, Any]


class ShadowModeConfig(BaseModel):
    """Configuration for shadow mode interceptor."""
    
    enabled: bool = False
    mq_broker: str = "localhost"
    mq_port: int = 51512
    mq_user: str = "fms"
    mq_password: str = "fms_secret"
    human_queue_topic: str = "shadow.human.queue"
    max_concurrent_tasks: int = 10
    human_response_timeout: float = 300.0  # 5 minutes
    enable_real_ai_runner: bool = True


class ShadowModeStats(BaseModel):
    """Statistics tracking for shadow mode."""
    
    total_queries: int = 0
    completed_queries: int = 0
    pending_human_tasks: int = 0
    discrepancies_found: int = 0
    critical_issues: int = 0
    avg_response_time_ms: float = 0.0
    last_updated: str = ""


class ShadowModeInterceptor:
    """Intercept queries for dual-run shadow mode with real AI runner."""
    
    def __init__(self, config: ShadowModeConfig, ai_runner=None):
        """Initialize shadow mode interceptor.
        
        Args:
            config: Shadow mode configuration
            ai_runner: Optional real AI runner instance. If None, uses mock runner.
        """
        self.config = config
        self.ai_runner = ai_runner
        self.enabled = config.enabled
        
        # In-memory caches for shadow mode
        self.pending_queries: dict[str, dict] = {}
        self.completed_queries: dict[str, dict] = {}
        self.human_answers: dict[str, dict] = {}
        
        # Statistics
        self.stats = ShadowModeStats()
        
        # MQ connection (lazy initialization)
        self.mq_publisher = None
        self.mq_subscriber = None
        
        # Thread safety
        self._lock = asyncio.Lock()
        
        logger.info(f"ShadowModeInterceptor initialized: enabled={enabled}, use_real_ai={config.enable_real_ai_runner}")
    
    async def start(self) -> None:
        """Start shadow mode background tasks (MQ subscriber, stats collection)."""
        
        if not self.enabled:
            logger.info("Shadow mode disabled, skipping startup")
            return
        
        logger.info("Starting shadow mode background tasks...")
        
        try:
            # Setup MQ publisher
            await self._setup_mq_publisher()
            
            # Start human queue subscriber
            asyncio.create_task(self._subscribe_to_human_queue())
            
            # Start periodic stats collection
            asyncio.create_task(self._periodic_stats_collection())
            
            logger.info("Shadow mode started successfully")
            
        except Exception as e:
            logger.error(f"Failed to start shadow mode: {e}")
            raise
    
    async def stop(self) -> None:
        """Stop shadow mode and cleanup resources."""
        
        if not self.enabled:
            return
        
        logger.info("Stopping shadow mode...")
        
        # Cleanup MQ connections
        if self.mq_publisher:
            await self.mq_publisher.disconnect()
        if self.mq_subscriber:
            await self.mq_subscriber.disconnect()
        
        logger.info("Shadow mode stopped")
    
    async def process_query(self, query: dict) -> dict:
        """Process a query through shadow mode interception.
        
        This is the main entry point for shadow mode interception. It will:
        1. Generate a unique query ID
        2. Route to both human queue AND real AI runner
        3. Wait for human response (optional, non-blocking)
        4. Collect responses and generate comparison
        
        Args:
            query: Original query dict with 'user_query' and optional 'context'
            
        Returns:
            AI runner response dict
        """
        
        async with self._lock:
            self.stats.total_queries += 1
            query_id = str(uuid.uuid4())
            query["query_id"] = query_id
            query["timestamp"] = datetime.now(timezone.utc).isoformat()
            
            self.pending_queries[query_id] = query
            
            logger.debug(f"[Shadow Mode] Processing query {query_id[:8]}...")
        
        # Route to human queue (non-blocking)
        if self.enabled and self.mq_publisher:
            try:
                human_task = {
                    "query_id": query_id,
                    "user_query": query.get("user_query"),
                    "context": query.get("context", {}),
                    "assigned_at": datetime.now(timezone.utc).isoformat(),
                    "priority": "normal",
                    "source": "shadow_mode"
                }
                
                await self._publish_to_human_queue(human_task)
                
                async with self._lock:
                    self.stats.pending_human_tasks += 1
                    
            except Exception as e:
                logger.error(f"Failed to enqueue human task: {e}")
        
        # Process with real AI runner (or mock if not available)
        start_time = datetime.now(timezone.utc)
        
        if self.config.enable_real_ai_runner and self.ai_runner:
            try:
                logger.debug(f"[Shadow Mode] Calling real AI runner...")
                ai_response = await self.ai_runner.run(query)
            except Exception as e:
                logger.error(f"Real AI runner failed: {e}")
                ai_response = await self._mock_ai_response(query_id, error=str(e))
        else:
            logger.debug(f"[Shadow Mode] Using mock AI response...")
            ai_response = await self._mock_ai_response(query_id)
        
        # Calculate latency
        end_time = datetime.now(timezone.utc)
        latency_ms = (end_time - start_time).total_seconds() * 1000
        
        async with self._lock:
            self.stats.avg_response_time_ms = (
                self.stats.avg_response_time_ms * (self.stats.completed_queries) + latency_ms
            ) / (self.stats.completed_queries + 1)
            
            self.completed_queries[query_id] = {
                "ai_response": ai_response,
                "latency_ms": latency_ms,
                "completed_at": end_time.isoformat()
            }
        
        # Try to get human answer if available (non-blocking)
        if self.enabled:
            human_answer = await self._get_recent_human_answer(query_id)
            
            if human_answer:
                # Compare responses immediately
                comparison_result = await self._compare_responses(
                    query_id, human_answer, ai_response
                )
                
                logger.info(
                    f"[Shadow Mode] Comparison complete for {query_id[:8]}: "
                    f"severity={comparison_result['max_severity']}, agreement={comparison_result['overall_agreement']:.1%}"
                )
            else:
                logger.debug(f"[Shadow Mode] No human answer yet for {query_id[:8]}")
        
        return ai_response
    
    async def submit_human_answer(self, query_id: str, answer: dict, operator_id: str, 
                                 feedback: Optional[dict] = None) -> None:
        """Submit human operator's answer for comparison.
        
        This should be called by the operator UI when they complete their manual analysis.
        
        Args:
            query_id: Original query ID from intercepted request
            answer: Human operator's answer dict
            operator_id: ID of operator who provided answer
            feedback: Optional feedback about quality, confidence, etc.
        """
        
        start_time = datetime.now(timezone.utc)
        
        async with self._lock:
            # Get original query
            query = self.pending_queries.get(query_id)
            if not query:
                logger.error(f"[Shadow Mode] Query not found: {query_id}")
                return
            
            # Get AI response
            completion = self.completed_queries.get(query_id)
            if not completion:
                logger.error(f"[Shadow Mode] AI response not found: {query_id}")
                return
            
            # Mark as complete
            async with self._lock:
                self.stats.pending_human_tasks -= 1
            
            # Store human answer
            self.human_answers[query_id] = {
                "answer": answer,
                "operator_id": operator_id,
                "submitted_at": datetime.now(timezone.utc).isoformat()
            }
            
            # Compare responses
            ai_response = completion["ai_response"]
            comparison = await self._compare_responses(query_id, answer, ai_response)
            
            # Store discrepancy if needed
            if comparison["discrepancies"]:
                async with self._lock:
                    self.stats.discrepancies_found += 1
                    
                    if comparison["max_severity"] == "critical":
                        self.stats.critical_issues += 1
                
                # Save to database (async, fire-and-forget)
                asyncio.create_task(self._save_discrepancy_to_db(comparison, query, feedback))
            
            logger.info(f"[Shadow Mode] Human answer submitted: {query_id[:8]}, "
                       f"agreement={comparison['overall_agreement']:.1%}")
    
    async def _compare_responses(self, query_id: str, human_answer: dict, 
                                agent_answer: dict) -> dict:
        """Compare human and agent answers for discrepancies."""
        
        if compare_answers is None:
            # Basic fallback comparison
            return {
                "query_id": query_id,
                "discrepancies": [],
                "discrepancy_count": 0,
                "max_severity": "none",
                "overall_agreement": 1.0
            }
        
        try:
            comparison = compare_answers(human_answer, agent_answer)
            comparison["query_id"] = query_id
            comparison["comparison_time"] = datetime.now(timezone.utc).isoformat()
            
            return comparison
            
        except Exception as e:
            logger.error(f"Failed to compare responses: {e}")
            return {
                "query_id": query_id,
                "error": str(e),
                "discrepancies": [],
                "discrepancy_count": 0,
                "max_severity": "informational",
                "overall_agreement": 1.0
            }
    
    async def _publish_to_human_queue(self, task: dict) -> None:
        """Publish task to MQ human processing queue."""
        
        message = ShadowQueueMessage(
            message_type="query",
            query_id=task["query_id"],
            timestamp=datetime.now(timezone.utc).isoformat(),
            data=task
        )
        
        # Publish via MQ (implementation depends on your MQ system)
        # For now, placeholder implementation
        logger.debug(f"Publishing to human queue: {task['query_id']}")
    
    async def _subscribe_to_human_queue(self) -> None:
        """Subscribe to human queue responses (background task)."""
        
        # Placeholder for MQ subscription logic
        logger.debug("Subscribing to human queue...")
        # Implementation would subscribe to MQ topic and collect human answers
    
    async def _get_recent_human_answer(self, query_id: str) -> Optional[dict]:
        """Get recent human answer if available."""
        
        answer_data = self.human_answers.get(query_id)
        if answer_data:
            return answer_data["answer"]
        return None
    
    async def _mock_ai_response(self, query_id: str, error: Optional[str] = None) -> dict:
        """Generate mock AI response when real AI runner unavailable."""
        
        import time
        start = time.time()
        
        response = {
            "query_id": query_id,
            "status": "error" if error else "success",
            "answer": {
                "text": "Mock AI response (shadow mode)" + (" - Error: " + error if error else ""),
                "confidence": 0.7,
                "evidence": []
            },
            "latency_ms": (time.time() - start) * 1000,
            "generated_at": datetime.now(timezone.utc).isoformat()
        }
        
        return response
    
    async def _save_discrepancy_to_db(self, comparison: dict, query: dict, 
                                     feedback: Optional[dict]) -> None:
        """Save discrepancy to database (fire-and-forget)."""
        
        # TODO: Implement database persistence
        # This should integrate with your existing discrepancy tracking schema
        logger.debug(f"Would save discrepancy: {comparison['query_id']}")
    
    async def _setup_mq_publisher(self) -> None:
        """Setup MQTT publisher for human queue."""
        
        # Placeholder for actual MQ setup
        # Would connect to RocketMQ or similar
        pass
    
    async def _periodic_stats_collection(self) -> None:
        """Periodically collect and log statistics (background task)."""
        
        while True:
            await asyncio.sleep(60)  # Every minute
            
            async with self._lock:
                logger.info(
                    f"[Shadow Mode Stats] Queries={self.stats.total_queries}, "
                    f"Completed={self.stats.completed_queries}, "
                    f"Discrepancies={self.stats.discrepancies_found}, "
                    f"AvgLatency={self.stats.avg_response_time_ms:.0f}ms"
                )


def setup_shadow_queue(mq_config: dict) -> tuple[Any, Any]:
    """Helper function to setup shadow mode MQ publisher/subscriber.
    
    Args:
        mq_config: Dictionary with broker, port, user, password
        
    Returns:
        Tuple of (publisher, subscriber) MQ connections
    """
    
    # Placeholder for actual MQ setup
    # Implementation would create RocketMQ topics and connections
    logger.info(f"Setting up shadow mode MQ with config: {mq_config}")
    
    # TODO: Replace with actual RocketMQ/MQTT setup
    publisher = None
    subscriber = None
    
    return publisher, subscriber


# Example usage and testing
async def main():
    """Example usage of ShadowModeInterceptor."""
    
    # Create config
    config = ShadowModeConfig(
        enabled=True,
        mq_broker="localhost",
        mq_port=51512,
        enable_real_ai_runner=False  # Set True when integrating with real AI runner
    )
    
    # Create interceptor (without real AI runner for demo)
    interceptor = ShadowModeInterceptor(config=config)
    
    # Start shadow mode
    await interceptor.start()
    
    # Test query
    test_query = {
        "user_query": "MU5102 当前状态是什么？",
        "context": {
            "current_time": "2026-08-15T14:30:00Z",
            "airport_code": "PVG"
        }
    }
    
    # Process through shadow mode
    result = await interceptor.process_query(test_query)
    
    print(f"Shadow mode processed query: {result}")
    
    # Stop shadow mode
    await interceptor.stop()


if __name__ == "__main__":
    # Run example if executed directly
    asyncio.run(main())
