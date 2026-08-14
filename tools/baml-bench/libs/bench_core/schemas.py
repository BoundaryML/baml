"""Shared pydantic models used across services and the proxy contract."""

from __future__ import annotations

from typing import Any, Literal, Optional

from pydantic import BaseModel, Field

# ---------- pricing ----------


class Prices(BaseModel):
    """Per-million-token USD rate card for a single model."""

    input_per_million_usd: float = 0.0
    output_per_million_usd: float = 0.0
    cache_read_per_million_usd: float = 0.0
    cache_write_per_million_usd: float = 0.0


# ---------- proxy /run-agent contract (ported from claude-proxy) ----------


class RunAgentRequest(BaseModel):
    """Request to spawn a claude agent against a cell on the version-cached baml CLI."""

    cell_id: str
    model: str = "claude-sonnet-4-6"
    max_turns: int = 40
    prompt: str
    system_prompt: Optional[str] = None
    files: dict[str, str] = Field(default_factory=dict)
    prices: Optional[Prices] = None
    # sha of the baml CLI the proxy should place on PATH (version-cache).
    baml_version: Optional[str] = None
    post_file_patterns: list[str] = Field(default_factory=list)
    max_file_bytes: int = 256 * 1024
    max_total_file_bytes: int = 4 * 1024 * 1024
    invocation_timeout_secs: Optional[int] = None


class CheckBamlRequest(BaseModel):
    """Request to run a baml command against a minimal repro on the version-cached CLI.

    No claude agent is spawned; the proxy just compiles/runs the supplied repro.
    """

    check_id: str
    files: dict[str, str] = Field(default_factory=dict)
    command: str = "baml generate"
    baml_version: Optional[str] = None
    timeout_secs: int = 90


class CheckBamlResult(BaseModel):
    """Outcome of a CheckBamlRequest: exit code, timeout flag, and output tails."""

    check_id: str
    exit_code: int = 0
    timed_out: bool = False
    baml_on_path: bool = False
    stdout_tail: Optional[str] = None
    stderr_tail: Optional[str] = None


class AgentResult(BaseModel):
    """Result of a run-agent invocation: status, token/cost metrics, and posted files."""

    cell_id: str
    status: str = "error"  # ok | timeout | error
    exit_code: int = 0
    transcript: Optional[str] = None
    stderr_tail: Optional[str] = None
    turns: Optional[int] = None
    tool_calls: Optional[int] = None
    api_calls: Optional[int] = None
    input_tokens: Optional[int] = None
    output_tokens: Optional[int] = None
    total_tokens: Optional[int] = None
    cache_read_tokens: Optional[int] = None
    cache_write_tokens: Optional[int] = None
    wall_clock_ms: Optional[int] = None
    estimated_cost_usd: Optional[float] = None
    turn_log: Optional[list[Any]] = None
    post_files: dict[str, str] = Field(default_factory=dict)
    host_metadata: Optional[dict[str, Any]] = None


# ---------- trophy metric bag (ported from suite_b_results) ----------


class Metrics(BaseModel):
    """Aggregated trophy metric bag rolled up across a run's invocations."""

    turns: Optional[int] = None
    tool_calls: Optional[int] = None
    api_calls: Optional[int] = None
    invocations: Optional[int] = None
    input_tokens: Optional[int] = None
    output_tokens: Optional[int] = None
    total_tokens: Optional[int] = None
    cache_read_tokens: Optional[int] = None
    cache_write_tokens: Optional[int] = None
    wall_clock_ms: Optional[int] = None
    setup_build_ms: Optional[int] = None
    agent_thinking_ms: Optional[int] = None
    debug_retries: Optional[int] = None
    estimated_cost_usd: Optional[float] = None
    files_touched: Optional[int] = None
    loc_changed: Optional[int] = None


# ---------- evidence / findings (worker self-report -> baml-dedup -> issue) ----------

Kind = Literal["skill", "language"]


class EvidenceAnchor(BaseModel):
    """Pointer back to the trophy and transcript location that a finding cites."""

    trophy_id: Optional[str] = None
    turn_index: Optional[int] = None
    call_index: Optional[int] = None
    note: Optional[str] = None


class Finding(BaseModel):
    """A single skill or language issue the worker agent surfaced in a run, with its transcript anchor."""

    kind: Kind
    title: str
    description: str
    # the worker cites the exact transcript turn where this surfaced
    call_index: Optional[int] = None
    turn_index: Optional[int] = None
    repro: Optional[str] = None
