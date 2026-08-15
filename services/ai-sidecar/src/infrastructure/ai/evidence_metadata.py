"""P1-1-A: Evidence Metadata Chain helpers.

Provides utilities for building and validating evidence metadata including
source/object_id/as_of/freshness tracking for all query tool outputs.
"""

from __future__ import annotations

import hashlib
import time
from datetime import datetime, timezone
from typing import Any


def build_evidence_metadata(
    *,
    source: str,
    object_type: str,
    object_id: str,
    freshness_seconds: int | None = None,
    as_of: datetime | None = None,
) -> dict[str, Any]:
    """Build evidence metadata dictionary with P1-1-A requirements.
    
    Args:
        source: Data source identifier (e.g., "ai_query.flight_status")
        object_type: Type of object (e.g., "flight", "stand", "dispatch_order")
        object_id: Unique object identifier
        freshness_seconds: How fresh the data is (optional - will be computed if not provided)
        as_of: Timestamp when data was valid (defaults to current UTC time)
    
    Returns:
        Dict containing:
        {
            "source": str,
            "object_type": str,
            "object_id": str,
            "as_of": str (ISO8601),
            "freshness_seconds": int (if provided)
        }
    """
    # Normalize as_of timestamp
    if as_of is None:
        as_of = datetime.now(timezone.utc)
    elif isinstance(as_of, str):
        as_of = datetime.fromisoformat(as_of.replace("Z", "+00:00"))
    
    as_of_iso = as_of.isoformat()
    
    result: dict[str, Any] = {
        "source": source,
        "object_type": object_type,
        "object_id": object_id,
        "as_of": as_of_iso,
    }
    
    # Add freshness if provided or computable
    if freshness_seconds is not None:
        result["freshness_seconds"] = freshness_seconds
    
    return result


def generate_object_id(*, object_type: str, identifier: str, timestamp: datetime | None = None) -> str:
    """Generate deterministic object ID from type + identifier + optional timestamp.
    
    Example: "flight_MU5102_2026-08-15T14:30:00Z"
    
    Args:
        object_type: Type of object (flight, stand, dispatch_order)
        identifier: Business identifier (flight number, gate code)
        timestamp: Optional timestamp for temporal uniqueness
    
    Returns:
        Formatted object_id string
    """
    ts_str = ""
    if timestamp is not None:
        if isinstance(timestamp, str):
            dt = datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
        else:
            dt = timestamp
        ts_str = "_" + dt.strftime("%Y%m%dT%H%M%SZ").replace("+00:00", "")
    
    # Sanitize identifiers
    safe_identifier = str(identifier).strip().upper().replace("-", "_").replace("/", "_")
    
    return f"{object_type}_{safe_identifier}{ts_str}"


def compute_freshness_seconds(as_of: datetime, now: datetime | None = None) -> int:
    """Compute freshness in seconds from as_of timestamp.
    
    Args:
        as_of: When the data was valid
        now: Current time (defaults to datetime.now(timezone.utc))
    
    Returns:
        Freshness in seconds (integer)
    """
    if now is None:
        now = datetime.now(timezone.utc)
    
    if isinstance(now, str):
        now = datetime.fromisoformat(now.replace("Z", "+00:00"))
    if isinstance(as_of, str):
        as_of = datetime.fromisoformat(as_of.replace("Z", "+00:00"))
    
    delta = now - as_of
    return max(0, int(delta.total_seconds()))


# =============================================================================
# Freshness Validators by Tool Type
# =============================================================================

class FreshnessValidator:
    """Per-tool freshness threshold validators."""
    
    # P1-1-B: Define max_age thresholds per tool
    MAX_AGE_THRESHOLDS: dict[str, int] = {
        "flights.lookup": 30,      # Flight status changes rapidly
        "stands.current": 10,      # Gate assignments very dynamic
        "dispatch_orders": 60,     # Task assignments slightly less urgent
        "kpi_snapshot": 300,       # Aggregated metrics can be older
        "anomaly_detection": 120,  # Anomalies checked periodically
        "team_availability": 60,   # Staff availability updates hourly
    }
    
    @classmethod
    def get_max_age(cls, tool_name: str) -> int:
        """Get maximum allowed age in seconds for a tool's data."""
        return cls.MAX_AGE_THRESHOLDS.get(tool_name, 300)  # Default 5 minutes
    
    @classmethod
    def is_data_stale(
        cls, 
        tool_name: str, 
        as_of: datetime | str | None, 
        now: datetime | str | None = None
    ) -> tuple[bool, float]:
        """Check if data is stale based on tool-specific thresholds.
        
        Returns:
            Tuple of (is_stale, freshness_ratio) where freshness_ratio is 0.0-1.0
            (0.0 = fresh, >1.0 = stale)
        """
        max_age = cls.get_max_age(tool_name)
        
        if as_of is None:
            return True, float('inf')  # No timestamp = definitely stale
        
        freshness_seconds = compute_freshness_seconds(as_of, now)
        freshness_ratio = freshness_seconds / max_age if max_age > 0 else float('inf')
        
        return freshness_seconds > max_age, freshness_ratio
    
    @classmethod
    def validate_evidence(
        cls,
        evidence: list[dict[str, Any]],
        tool_name: str,
        now: datetime | str | None = None
    ) -> tuple[list[dict[str, Any]], list[str]]:
        """Validate evidence chain, filtering out stale entries.
        
        Returns:
            Tuple of (valid_evidence, warnings)
        """
        valid_evidence = []
        warnings = []
        
        for evd in evidence:
            as_of = evd.get("as_of")
            is_stale, ratio = cls.is_data_stale(tool_name, as_of, now)
            
            if is_stale:
                warnings.append(f"Evidence {evd.get('object_id')} stale (ratio={ratio:.2f})")
            else:
                valid_evidence.append(evd)
        
        return valid_evidence, warnings


# =============================================================================
# Confidence Scoring Helpers (P1-1-C)
# =============================================================================

def compute_confidence_score(
    *,
    has_source: bool = True,
    has_object_id: bool = True,
    has_as_of: bool = True,
    is_fresh: bool = True,
    completeness: float = 1.0,
    required_fields_missing: int = 0,
) -> float:
    """Compute confidence score for a piece of evidence/data.
    
    P1-1-C: Return uncertainty markers when confidence < 0.7
    
    Args:
        has_source: Evidence includes source field
        has_object_id: Evidence includes object_id
        has_as_of: Evidence includes as_of timestamp
        is_fresh: Data passes freshness check
        completeness: Completeness ratio (0.0-1.0)
        required_fields_missing: Number of missing required fields
    
    Returns:
        Confidence score between 0.0 and 1.0
    """
    score = 0.0
    
    # Base scores
    if has_source:
        score += 0.25
    if has_object_id:
        score += 0.25
    if has_as_of:
        score += 0.25
    if is_fresh:
        score += 0.15
    
    # Completeness factor
    score *= completeness
    
    # Penalty for missing required fields
    score -= required_fields_missing * 0.1
    
    return max(0.0, min(1.0, score))


def format_uncertainty_markdown(confidence: float, missing_fields: list[str] | None = None) -> str:
    """Format uncertainty warning in markdown for LLM consumption.
    
    P1-1-C: Generate human-readable uncertainty messages when confidence low.
    """
    if confidence >= 0.7:
        return ""  # High enough confidence, no warning needed
    
    warnings = []
    
    if confidence < 0.5:
        warnings.append("⚠️ **Low confidence** - Data uncertain, requires human verification")
    elif confidence < 0.7:
        warnings.append("⚠️ **Moderate confidence** - Some aspects may need verification")
    
    if missing_fields:
        warnings.append(f"- Missing/unknown fields: {', '.join(missing_fields)}")
    
    return "\n".join(warnings)
