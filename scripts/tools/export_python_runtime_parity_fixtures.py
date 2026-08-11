#!/usr/bin/env python
"""Export deterministic runtime parity fixtures used by Rust API tests.

The Python HTTP backend has been retired.  These fixtures preserve the stable
Python-compatible runtime payload contracts without importing or monkey-patching
deprecated Python application modules.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


TOPICS = {
    "MU100": 1,
    "flight_updates": 1,
}

CLIENT_BUFFER = {
    "client_id": "client-buffer",
    "is_active": True,
    "queue_size": 1,
    "queue_maxsize": 64,
    "queue_full": False,
    "subscriptions": ["MU100", "flight_updates"],
}


def verification_health_payload(*, redis_connected: bool, token_configured: bool) -> dict[str, Any]:
    return {
        "success": True,
        "data": {
            "verification_service": "running",
            "redis_connected": redis_connected,
            "token_configured": token_configured,
        },
        "message": "Verification service health check",
    }


def buffer_status_summary() -> dict[str, Any]:
    return {
        "total_connections": 1,
        "total_queue_size": 1,
        "total_queue_capacity": 64,
        "buffer_utilization_percent": 1.56,
        "queue_full_count": 0,
        "topics": TOPICS,
        "status": "active",
        "timestamp": "<timestamp>",
    }


def buffer_status_detailed() -> dict[str, Any]:
    return {
        **buffer_status_summary(),
        "client_buffers": [CLIENT_BUFFER],
        "client_buffers_returned": 1,
        "client_buffers_total": 1,
        "client_buffers_truncated": False,
    }


def buffer_status_flight_hit() -> dict[str, Any]:
    return {
        "flight_no": "MU100",
        "status": "active",
        "subscribed_clients": 1,
        "total_queue_size": 1,
        "client_buffers": [CLIENT_BUFFER],
        "timestamp": "<timestamp>",
    }


def buffer_status_flight_miss() -> dict[str, Any]:
    return {
        "flight_no": "CZ200",
        "status": "not_in_buffer",
        "message": "Flight CZ200 is not currently in any client's subscription",
        "subscribed_clients": 0,
        "total_queue_size": 0,
        "suggestion": "Check if the flight exists or if clients are subscribed to flight_updates topic",
        "all_connections_count": 1,
        "all_topics": TOPICS,
        "timestamp": "<timestamp>",
    }


def sse_stats_single_connection() -> dict[str, Any]:
    return {
        "active_connections": 1,
        "total_connections": 1,
        "active_connections_gauge": 1,
        "lifetime_connections": 1,
        "lifetime_connections_counter": 1,
        "messages_sent": 2,
        "messages_failed": 0,
        "messages_dropped": 0,
        "topics": TOPICS,
        "connection_breakdown": {
            "connected": 1,
            "inactive": 0,
        },
        "connection_details": [
            {
                "client_id": "client-buffer",
                "is_active": True,
                "last_heartbeat": "<timestamp>",
                "time_since_heartbeat": "<duration>",
                "queue_size": 1,
                "queue_full": False,
                "dropped_messages": 0,
                "subscriptions": ["MU100", "flight_updates"],
            }
        ],
        "heartbeat_interval": 15,
        "max_connections": 1000,
        "connection_queue_size": 64,
        "cleanup_interval_seconds": 30,
        "heartbeat_timeout_seconds": 45,
        "queue_full_disconnect_seconds": 10,
    }


def performance_metrics_sample() -> dict[str, Any]:
    return {
        "db_pool": {
            "active": 3,
            "idle": 1,
            "max": 10,
            "usage_pct": 30.0,
        },
        "redis": {
            "latency_ms": 12.34,
            "connected": True,
        },
        "sse": {
            "connections": 1,
            "max": 1000,
            "usage_pct": 0.1,
        },
        "requests": {
            "p50": 20.0,
            "p95": 29.0,
            "p99": 29.8,
            "avg": 20.0,
            "count": 3,
        },
        "auth": {
            "login_success": 1,
            "login_failure": 1,
            "login_total": 2,
            "login_success_rate_pct": 50.0,
            "refresh_success": 1,
            "refresh_failure": 1,
            "refresh_total": 2,
            "refresh_success_rate_pct": 50.0,
            "session_lost": 1,
            "logout_total": 1,
            "heartbeat_total": 4,
        },
        "notification_delivery": {
            "push_attempts": 0,
            "push_success": 0,
            "push_success_rate_pct": 0.0,
            "sse_attempts": 3,
            "sse_success": 2,
            "sse_success_rate_pct": 66.67,
            "external_attempts": 0,
            "external_success": 0,
            "in_app_attempts": 0,
            "in_app_success": 0,
            "backfill_pending": 1,
        },
        "mobile_realtime": {
            "sse_reconnects": 2,
        },
        "timestamp": "<timestamp>",
    }


def health_status_cases() -> dict[str, str]:
    return {
        "healthy": "healthy",
        "degraded_on_errors": "degraded",
        "degraded_on_inactive_sse": "degraded",
        "down_on_postgres": "down",
    }


def build_fixtures() -> dict[str, Any]:
    return {
        "buffer_status_summary_single_connection": buffer_status_summary(),
        "buffer_status_detailed_single_connection": buffer_status_detailed(),
        "buffer_status_flight_hit_single_connection": buffer_status_flight_hit(),
        "buffer_status_flight_miss_single_connection": buffer_status_flight_miss(),
        "sse_stats_single_connection": sse_stats_single_connection(),
        "performance_metrics_sample": performance_metrics_sample(),
        "health_status_cases": health_status_cases(),
        "verification_health_token_unset": verification_health_payload(
            redis_connected=False,
            token_configured=False,
        ),
        "verification_health_token_set": verification_health_payload(
            redis_connected=False,
            token_configured=True,
        ),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Export Python runtime parity fixtures as JSON.",
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="Path to write the fixture JSON file.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(build_fixtures(), ensure_ascii=False, indent=2, sort_keys=True),
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
