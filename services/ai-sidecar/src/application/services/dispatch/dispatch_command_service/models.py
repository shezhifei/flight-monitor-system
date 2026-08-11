"""Module-level constants for the dispatch command service.

Class-level constants are defined here so they can be shared and tested
independently of the service class.  ``DispatchCommandApplicationService``
re-binds them as class attributes to preserve ``self.<CONSTANT>`` access
without altering method bodies.
"""

from __future__ import annotations

CHECKIN_DISTANCE_THRESHOLD_METERS = 300.0

ACTION_LOG_MAP = {
    "accept": "accepted",
    "checkin": "checkin",
    "checkout": "checkout",
    "start": "started",
    "complete": "completed",
    "eta_report": "estimated_completion_reported",
    "report_issue": "issue_reported",
}
