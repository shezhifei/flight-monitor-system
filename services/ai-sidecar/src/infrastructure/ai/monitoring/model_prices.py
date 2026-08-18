"""LLM pricing table used for run-cost accounting (Task J1).

Prices are USD per 1M tokens, keyed by model-family prefix. Lookup is
longest-prefix; unknown models cost zero and report ``price_missing=1``
so dashboards can flag runs whose cost is not trustworthy.

Keep entries coarse (family level) — the metric label cardinality comes
from ``task_type`` / ``entity_id``, not from exact model versions.
"""

from __future__ import annotations

# USD per 1M tokens: (prompt, completion). Longest matching prefix wins.
MODEL_PRICES_PER_1M: dict[str, tuple[float, float]] = {
    "gpt-4o-mini": (0.15, 0.60),
    "gpt-4o": (2.50, 10.00),
    "gpt-4-turbo": (10.00, 30.00),
    "gpt-4.1-mini": (0.40, 1.60),
    "gpt-4.1": (2.00, 8.00),
    "gpt-5-mini": (0.25, 2.00),
    "gpt-5": (1.25, 10.00),
    "o3-mini": (1.10, 4.40),
    "o3": (2.00, 8.00),
    "o4-mini": (1.10, 4.40),
}


def lookup_price_per_1m(model: str) -> tuple[float, float] | None:
    """Return ``(prompt, completion)`` USD per 1M tokens for ``model``.

    Longest-prefix match; ``None`` when no family matches (unknown model).
    """
    name = str(model or "").strip().lower()
    if not name:
        return None
    best_prefix = ""
    best_price: tuple[float, float] | None = None
    for prefix, price in MODEL_PRICES_PER_1M.items():
        if name.startswith(prefix) and len(prefix) > len(best_prefix):
            best_prefix = prefix
            best_price = price
    return best_price


def estimate_run_cost_usd(
    model: str,
    prompt_tokens: int,
    completion_tokens: int,
) -> tuple[float, bool]:
    """Estimate the USD cost of one LLM call.

    Returns ``(cost_usd, price_missing)``. Unknown models return
    ``(0.0, True)`` — cost accounting must never overstate spend.
    """
    price = lookup_price_per_1m(model)
    if price is None:
        return 0.0, True
    prompt_price, completion_price = price
    cost = (max(0, int(prompt_tokens)) * prompt_price + max(0, int(completion_tokens)) * completion_price) / 1_000_000.0
    return cost, False
