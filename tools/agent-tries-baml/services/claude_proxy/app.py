"""claude-proxy - HTTP wrapper around the Claude Code CLI.

Single endpoint /run-agent: stage files, ensure the requested baml sha is
cached locally (pulled once from the service), optionally `baml agent install`
the official skills into the workspace, spawn claude with that baml on PATH,
parse the session, return transcript + metrics + post_files.
"""

from __future__ import annotations

import asyncio
import hmac
import logging
import os
import re
import shutil
import time
from pathlib import Path
from typing import Optional

import httpx
from fastapi import FastAPI, Header, HTTPException

from bench_core.schemas import (
    AgentResult,
    CheckBamlRequest,
    CheckBamlResult,
    RunAgentRequest,
)

from . import runner

log = logging.getLogger("claude_proxy")

PROXY_TOKEN = os.environ.get("ATB_CLAUDE_PROXY_TOKEN", "")
STAGING_ROOT = Path(os.environ.get("STAGING_ROOT", "/tmp/staging"))
BAML_CACHE_DIR = Path(os.environ.get("BAML_CACHE_DIR", "/var/baml-cache"))
CLAUDE_BIN = os.environ.get("CLAUDE_BIN", "claude")
ANTHROPIC_API_KEY = os.environ.get("ANTHROPIC_API_KEY", "")
# Auth mode for the spawned Claude Code CLI:
#   oauth   - never inject ANTHROPIC_API_KEY; the CLI uses the subscription
#             login persisted on the claude_state volume (~/.claude).
#   api-key - always inject ANTHROPIC_API_KEY (metered API billing).
#   auto    - oauth when a persisted login exists on the volume, else api-key.
CLAUDE_AUTH_MODE = os.environ.get("CLAUDE_AUTH_MODE", "auto").lower()
# The Claude Code CLI stores its OAuth state here after a one-time
# `fly ssh console -C "claude /login"` (the volume mount preserves it).
OAUTH_CREDENTIALS_PATH = Path(
    os.environ.get("CLAUDE_OAUTH_CREDENTIALS", str(Path.home() / ".claude" / ".credentials.json"))
)
DEFAULT_TIMEOUT = int(os.environ.get("CLAUDE_INVOCATION_TIMEOUT_SECS", "1800"))
# Bound on the pre-run `baml agent install` (fetches the skill archive over the
# network), so a wedged fetch can't eat into the agent's wall clock.
SKILL_INSTALL_TIMEOUT_SECS = int(os.environ.get("SKILL_INSTALL_TIMEOUT_SECS", "120"))
SERVICE_URL = os.environ.get("SERVICE_URL", "")
SERVICE_TOKEN = os.environ.get("ATB_SERVICE_TOKEN", "")
ALLOWED_MODELS = {
    "claude-sonnet-4-6",
    "claude-haiku-4-5",
    "claude-opus-4-7",
    "claude-opus-4-8",
}

_baml_locks: dict[str, asyncio.Lock] = {}

# Staging dir names are built from caller-supplied ids; restrict them to a safe
# charset so they cannot escape STAGING_ROOT via path separators or traversal.
_SAFE_ID = re.compile(r"^[A-Za-z0-9._-]+$")


def _safe_staging(prefix: str, raw: str, field: str) -> Path:
    """Resolve a staging directory for a caller-supplied id, rejecting traversal.

    Args:
        prefix: Literal prefix for the directory name (e.g. ``"check-"``).
        raw: The caller-supplied id (``cell_id`` / ``check_id``).
        field: Field name used in the error message.

    Returns:
        The staging path ``STAGING_ROOT / f"{prefix}{id}"``.

    Raises:
        HTTPException: 400 when the id is empty, too long, has disallowed
            characters, lacks an alphanumeric (e.g. ".", "..", all-punctuation),
            or resolves to anything other than a strict child of STAGING_ROOT.
    """
    rid = (raw or "").strip()
    # Whitelist the charset, cap the length (clean 400 instead of an OS error),
    # and require an alphanumeric so dots/punctuation-only names (".", "..", "...")
    # are rejected outright, regardless of prefix.
    if (not rid or len(rid) > 128 or not _SAFE_ID.match(rid)
            or not any(c.isalnum() for c in rid)):
        raise HTTPException(400, f"{field} required or invalid")
    base = STAGING_ROOT.resolve()
    p = (STAGING_ROOT / f"{prefix}{rid}").resolve()
    # Strict descendant only: belt-and-suspenders against the root itself.
    if p == base or not p.is_relative_to(base):
        raise HTTPException(400, f"{field} required or invalid")
    return p


def _auth_mode() -> str:
    """Resolve the effective auth mode for spawned CLI runs.

    Returns:
        "oauth" when runs should use the volume-persisted subscription login
        (no key injected), "api-key" when ANTHROPIC_API_KEY is injected, or
        "none" when neither source is available (runs will fail fast).
    """
    has_oauth = OAUTH_CREDENTIALS_PATH.exists()
    if CLAUDE_AUTH_MODE == "oauth":
        return "oauth" if has_oauth else "none"
    if CLAUDE_AUTH_MODE == "api-key":
        return "api-key" if ANTHROPIC_API_KEY else "none"
    # auto: prefer the subscription login when present, else the API key
    if has_oauth:
        return "oauth"
    return "api-key" if ANTHROPIC_API_KEY else "none"


def _get_api_key() -> str:
    """Return the API key to inject into a run, or "" for OAuth mode.

    An empty return makes the runner scrub ANTHROPIC_API_KEY from the spawned
    CLI's environment so it falls back to the persisted subscription login.
    """
    return ANTHROPIC_API_KEY if _auth_mode() == "api-key" else ""


def _require_bearer(authorization: str) -> None:
    """Reject the request unless its bearer token matches the proxy token.

    A no-op when no PROXY_TOKEN is configured (auth disabled).

    Args:
        authorization: The raw Authorization header value, e.g. "Bearer <token>".

    Raises:
        HTTPException: 403 when a token is configured and the header does not match.
    """
    if not PROXY_TOKEN:
        return
    expected = f"Bearer {PROXY_TOKEN}"
    if not hmac.compare_digest(authorization, expected):
        raise HTTPException(403, "forbidden")


async def _ensure_baml(sha: Optional[str]) -> Optional[Path]:
    """Return the dir containing the `baml` binary for `sha`, caching on miss.

    Fetches the binary from the service exactly once per sha (guarded by a
    per-sha lock) and caches it under BAML_CACHE_DIR. Degrades to None rather
    than failing when the binary is unavailable or no sha is requested.

    Args:
        sha: The baml version sha to provision, or None to skip provisioning.

    Returns:
        The directory containing the cached `baml` binary, or None when no sha
        was requested or the binary could not be fetched.
    """
    if not sha:
        return None
    bin_dir = BAML_CACHE_DIR / sha
    bin_path = bin_dir / "baml"
    if bin_path.exists():
        return bin_dir
    lock = _baml_locks.setdefault(sha, asyncio.Lock())
    async with lock:
        if bin_path.exists():
            return bin_dir
        if not SERVICE_URL:
            log.warning("baml_version=%s requested but SERVICE_URL unset; skipping", sha)
            return None
        bin_dir.mkdir(parents=True, exist_ok=True)
        headers = {"Authorization": f"Bearer {SERVICE_TOKEN}"} if SERVICE_TOKEN else {}
        try:
            async with httpx.AsyncClient(timeout=120.0) as c:
                r = await c.get(f"{SERVICE_URL}/baml/binary/{sha}", headers=headers)
                r.raise_for_status()
                bin_path.write_bytes(r.content)
            bin_path.chmod(0o755)
        except Exception as e:  # noqa: BLE001
            # Binary not available yet (build pending / blob missing). Degrade
            # gracefully: run the agent WITHOUT canary baml on PATH rather than
            # failing the whole run. The agent can still install baml itself.
            log.warning("baml %s unavailable (%s); running without it", sha, e)
            return None
        log.info("cached baml %s (%d bytes)", sha, bin_path.stat().st_size)
    return bin_dir


async def _install_skills(staging: Path, baml_dir: Optional[Path], cell_id: str) -> bool:
    """Install the official BAML agent skills into the staging dir.

    Runs `baml agent install --dir .` with the version-cached baml on PATH, so
    the agent finds the skills at .claude/skills/baml-* exactly as a real user
    would after running the command. Degrades to False (run proceeds without
    skills) rather than failing the run, mirroring how a missing baml binary is
    handled.

    Args:
        staging: The staging directory the skills are installed into.
        baml_dir: Directory containing the cached `baml` binary, or None when
            the binary is unavailable (install is skipped).
        cell_id: The run's cell id, for log attribution.

    Returns:
        True when the install completed cleanly, else False.
    """
    if baml_dir is None:
        log.warning("install_skill requested for %s but no baml binary; skipping", cell_id)
        return False
    stdout, stderr, exit_code, timed_out = await runner.run_command(
        cwd=staging,
        command="baml agent install --dir .",
        baml_bin_dir=baml_dir,
        timeout_secs=SKILL_INSTALL_TIMEOUT_SECS,
    )
    if exit_code != 0:
        log.warning(
            "baml agent install failed for %s (exit=%d, timed_out=%s): %s",
            cell_id, exit_code, timed_out, (stderr or stdout)[-500:],
        )
        return False
    return True


app = FastAPI(title="agent-tries-baml-claude-proxy")


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    """Liveness probe that also reports the effective CLI auth mode.

    Returns:
        A dict with "status": "ok" and "auth": oauth | api-key | none, so the
        billing path is observable without shelling into the machine.
    """
    return {"status": "ok", "auth": _auth_mode()}


@app.post("/run-agent")
async def run_agent(req: RunAgentRequest, authorization: str = Header(default="")) -> AgentResult:
    """Run a Claude Code agent over staged files and return transcript + metrics.

    Stages the request's files, ensures the requested baml sha is on PATH,
    spawns claude, parses the session for turns/tokens/tool calls, optionally
    computes cost and collects post-run files, then cleans up the staging dir.

    Args:
        req: The run-agent request describing the cell, model, prompt, files,
            and limits.
        authorization: The raw Authorization header used for bearer auth.

    Returns:
        The populated AgentResult with status, transcript, metrics, and any
        collected post_files.

    Raises:
        HTTPException: 403 when the model is not allowed, or 400 when cell_id
            is missing.
    """
    _require_bearer(authorization)
    if req.model not in ALLOWED_MODELS:
        raise HTTPException(403, "model not allowed")
    staging = _safe_staging("", req.cell_id, "cell_id")
    out = AgentResult(cell_id=req.cell_id, status="error", host_metadata=runner.host_metadata())
    started = time.monotonic()
    try:
        runner.materialize_files(staging, req.files)
        baml_dir = await _ensure_baml(req.baml_version)
        if req.install_skill:
            out.skill_installed = await _install_skills(staging, baml_dir, req.cell_id)

        stdout, stderr, exit_code = await runner.spawn_claude(
            claude_bin=CLAUDE_BIN,
            cwd=staging,
            prompt=req.prompt,
            model=req.model,
            max_turns=req.max_turns,
            system_prompt=req.system_prompt,
            baml_bin_dir=baml_dir,
            timeout_secs=req.invocation_timeout_secs or DEFAULT_TIMEOUT,
            anthropic_api_key=_get_api_key(),
        )
        out.exit_code = exit_code
        out.transcript = stdout
        if exit_code != 0:
            tail = "\n".join(stderr.splitlines()[-20:])
            out.stderr_tail = tail
            out.status = "timeout" if exit_code == -9 else "error"
        else:
            out.status = "ok"

        session = runner.parse_claude_session(stdout)
        out.turns = session["turns"]
        out.tool_calls = session["tool_calls"]
        out.input_tokens = session["input_tokens"]
        out.output_tokens = session["output_tokens"]
        out.total_tokens = session["total_tokens"]
        out.cache_read_tokens = session["cache_read_tokens"]
        out.cache_write_tokens = session["cache_write_tokens"]
        if req.prices is not None:
            out.estimated_cost_usd = runner.compute_cost(session, req.prices.model_dump())

        sid = session.get("session_id")
        if sid:
            log_path = runner.session_log_path(staging, sid)
            if log_path and log_path.exists():
                log_text = log_path.read_text()
                # Persist the actual conversation as the transcript, rendered
                # terminal-style, instead of claude's --output-format json blob.
                out.transcript = runner.render_terminal_transcript(log_text)
                turn_log, api_calls = runner.parse_turn_log(log_text)
                if turn_log:
                    out.turn_log = turn_log
                if api_calls:
                    out.api_calls = api_calls
                # Claude's summary often omits num_tool_calls; derive it from
                # the per-turn tool list so the metric isn't null.
                if out.tool_calls is None and turn_log:
                    out.tool_calls = sum(len(t.get("tools") or []) for t in turn_log)

        if req.post_file_patterns:
            out.post_files = runner.collect_post_files(
                staging, req.post_file_patterns, req.max_file_bytes, req.max_total_file_bytes
            )
    except Exception as e:  # noqa: BLE001
        log.exception("/run-agent failed for %s", req.cell_id)
        if out.transcript is None:
            out.transcript = f"{e}"
    finally:
        out.wall_clock_ms = int((time.monotonic() - started) * 1000)
        shutil.rmtree(staging, ignore_errors=True)

    return out


@app.post("/check-baml")
async def check_baml(req: CheckBamlRequest, authorization: str = Header(default="")) -> CheckBamlResult:
    """Compile/run a minimal repro with the version-cached baml on PATH (no
    claude). Used by the worker to verify a finding's repro reproduces.

    Provisions a minimal baml.toml and generator block when the repro omits
    them, runs the requested command, and returns its exit code and output
    tails, cleaning up the staging dir afterward.

    Args:
        req: The check request describing the repro files, command, and timeout.
        authorization: The raw Authorization header used for bearer auth.

    Returns:
        The CheckBamlResult with exit code, timeout flag, and stdout/stderr tails.

    Raises:
        HTTPException: 400 when check_id is missing.
    """
    _require_bearer(authorization)
    staging = _safe_staging("check-", req.check_id, "check_id")
    out = CheckBamlResult(check_id=req.check_id)
    try:
        runner.materialize_files(staging, req.files)
        # baml commands require a project (baml.toml) and `generate` needs a
        # generator block. Provision minimal ones so the repro exercises real
        # compile/run behavior, not the project gate - unless the repro ships
        # its own. A type error still surfaces before generator resolution, so
        # this never masks a should_fail repro.
        if "baml.toml" not in req.files and not (staging / "baml.toml").exists():
            (staging / "baml.toml").write_text('[package]\nname = "repro"\n')
        if not any("generator " in c for c in req.files.values()):
            gen_dir = staging / "baml_src"
            gen_dir.mkdir(parents=True, exist_ok=True)
            (gen_dir / "_repro_generator.baml").write_text(
                'generator my_client {\n'
                '  output_type "python/pydantic"\n'
                '  output_dir ".."\n'
                '  naming_convention "preserve-case"\n'
                '}\n'
            )
        baml_dir = await _ensure_baml(req.baml_version)
        out.baml_on_path = baml_dir is not None
        stdout, stderr, exit_code, timed_out = await runner.run_command(
            cwd=staging,
            command=req.command,
            baml_bin_dir=baml_dir,
            timeout_secs=req.timeout_secs,
        )
        out.exit_code = exit_code
        out.timed_out = timed_out
        out.stdout_tail = stdout[-4000:]
        out.stderr_tail = stderr[-4000:]
    except Exception as e:  # noqa: BLE001
        log.exception("/check-baml failed for %s", req.check_id)
        out.exit_code = -1
        out.stderr_tail = str(e)
    finally:
        shutil.rmtree(staging, ignore_errors=True)
    return out
