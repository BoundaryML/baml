"""Price table loading + cost computation (ported from claude-proxy)."""

from __future__ import annotations

import tomllib
from pathlib import Path
from typing import Optional

from .schemas import Prices

_PRICES_PATH = Path(__file__).with_name("prices.toml")
_TABLE: dict[str, Prices] = {}


def _load() -> dict[str, Prices]:
    """Lazily load and memoize the model rate table from prices.toml.

    Returns:
        A mapping of model name to its Prices rate card, cached after the
        first call.
    """
    global _TABLE
    if not _TABLE:
        raw = tomllib.loads(_PRICES_PATH.read_text())
        _TABLE = {model: Prices(**vals) for model, vals in raw.items()}
    return _TABLE


def prices_for(model: str) -> Optional[Prices]:
    """Look up the rate card for a model.

    Args:
        model: The model name to look up.

    Returns:
        The model's Prices entry, or None if the model is not in the table.
    """
    return _load().get(model)


def compute_cost(
    *,
    input_tokens: int = 0,
    output_tokens: int = 0,
    cache_read_tokens: int = 0,
    cache_write_tokens: int = 0,
    prices: Prices,
) -> float:
    """Compute the USD cost of a token breakdown at the given per-million rates.

    Args:
        input_tokens: Number of prompt input tokens.
        output_tokens: Number of generated output tokens.
        cache_read_tokens: Number of tokens served from the prompt cache.
        cache_write_tokens: Number of tokens written to the prompt cache.
        prices: The per-million-token rate card to bill against.

    Returns:
        The total cost in USD.
    """
    return (
        input_tokens * prices.input_per_million_usd
        + output_tokens * prices.output_per_million_usd
        + cache_read_tokens * prices.cache_read_per_million_usd
        + cache_write_tokens * prices.cache_write_per_million_usd
    ) / 1_000_000.0
