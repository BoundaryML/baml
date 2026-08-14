"""Core claude invocation + session parsing, ported from claude-proxy.

Faithful port of spawn_claude / parse_claude_session / parse_turn_log /
compute_cost / file staging. The 1-based turn `i` index produced here is
what the worker cites as call_index, so it must match byte-for-byte.
"""

from __future__ import annotations

import asyncio
import json
import os
import platform
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

PREVIEW_CHARS = 400
# Error results get a larger budget: baml prints the real `error:` line after a
# stack of "Loading …baml" lines, so 400 head-chars often cuts it off. Keep both
# the head (the command echo) and the tail (where the error lands).
ERROR_PREVIEW_CHARS = 2000


# ---------- file staging ----------


def validate_relative_path(rel: str) -> None:
    """Reject paths that are empty, absolute, or contain parent traversal.

    Args:
        rel: The relative path to validate.

    Raises:
        ValueError: If the path is empty, absolute, or contains a ".." component.
    """
    if not rel:
        raise ValueError("empty path")
    p = Path(rel)
    if p.is_absolute():
        raise ValueError(f"absolute paths not allowed: {rel}")
    for part in p.parts:
        if part == "..":
            raise ValueError(f"bad path component in {rel}")


def materialize_files(staging: Path, files: dict[str, str]) -> None:
    """Write each file's content into the staging directory, creating parents.

    Args:
        staging: The root staging directory to write files under.
        files: A mapping of validated relative paths to their text content.
    """
    staging.mkdir(parents=True, exist_ok=True)
    for rel, content in files.items():
        validate_relative_path(rel)
        abs_path = staging / rel
        abs_path.parent.mkdir(parents=True, exist_ok=True)
        abs_path.write_text(content)


# ---------- claude spawning ----------


async def spawn_claude(
    *,
    claude_bin: str,
    cwd: Path,
    prompt: str,
    model: str,
    max_turns: int,
    system_prompt: Optional[str],
    baml_bin_dir: Optional[Path],
    timeout_secs: int,
    anthropic_api_key: str,
) -> tuple[str, str, int]:
    """Run `claude -p -` and return (stdout, stderr, exit_code).

    exit_code -9 signals a wall-clock timeout (SIGKILL), matching the
    Rust convention so callers can distinguish it from a clean non-zero.

    Args:
        claude_bin: Path or name of the claude CLI binary to invoke.
        cwd: Working directory (the staging dir) the agent runs in.
        prompt: The prompt fed to claude on stdin.
        model: The model identifier passed via --model.
        max_turns: Maximum agent turns passed via --max-turns.
        system_prompt: Optional system prompt appended via --append-system-prompt;
            ignored when empty or blank.
        baml_bin_dir: Optional directory prepended to PATH so the agent's `baml`
            calls hit the built sha.
        timeout_secs: Wall-clock timeout in seconds before the process is killed.
        anthropic_api_key: API key injected as ANTHROPIC_API_KEY; ignored when empty.

    Returns:
        A (stdout, stderr, exit_code) tuple; exit_code is -9 on timeout.
    """
    args = [
        claude_bin, "-p", "-",
        "--output-format", "json",
        "--max-turns", str(max_turns),
        "--model", model,
        "--permission-mode", "bypassPermissions",
    ]
    if system_prompt and system_prompt.strip():
        args += ["--append-system-prompt", system_prompt]

    env = os.environ.copy()
    if anthropic_api_key:
        env["ANTHROPIC_API_KEY"] = anthropic_api_key
    # Warm baml on PATH so the agent's `baml ...` calls hit the built sha.
    if baml_bin_dir is not None:
        env["PATH"] = f"{baml_bin_dir}:{env.get('PATH', '')}"

    proc = await asyncio.create_subprocess_exec(
        *args,
        cwd=str(cwd),
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=env,
    )

    timed_out = False
    try:
        stdout_b, stderr_b = await asyncio.wait_for(
            proc.communicate(input=prompt.encode()), timeout=timeout_secs
        )
        exit_code = proc.returncode if proc.returncode is not None else -1
    except asyncio.TimeoutError:
        timed_out = True
        proc.kill()
        stdout_b, stderr_b = await proc.communicate()
        exit_code = -9

    stdout = stdout_b.decode("utf-8", "replace")
    stderr = stderr_b.decode("utf-8", "replace")
    if timed_out:
        stderr += f"\n[proxy] claude killed after {timeout_secs}s wall-clock timeout\n"
    return stdout, stderr, exit_code


async def run_command(
    *,
    cwd: Path,
    command: str,
    baml_bin_dir: Optional[Path],
    timeout_secs: int,
) -> tuple[str, str, int, bool]:
    """Run a shell command (e.g. `baml build`) in cwd with the version-cached
    baml on PATH. Returns (stdout, stderr, exit_code, timed_out). exit_code -9
    signals a wall-clock timeout.

    Args:
        cwd: Working directory the command runs in.
        command: The shell command line to execute.
        baml_bin_dir: Optional directory prepended to PATH so `baml` resolves to
            the cached sha.
        timeout_secs: Wall-clock timeout in seconds before the process is killed.

    Returns:
        A (stdout, stderr, exit_code, timed_out) tuple; exit_code is -9 on timeout.
    """
    env = os.environ.copy()
    if baml_bin_dir is not None:
        env["PATH"] = f"{baml_bin_dir}:{env.get('PATH', '')}"
    proc = await asyncio.create_subprocess_shell(
        command,
        cwd=str(cwd),
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=env,
    )
    timed_out = False
    try:
        stdout_b, stderr_b = await asyncio.wait_for(proc.communicate(), timeout=timeout_secs)
        exit_code = proc.returncode if proc.returncode is not None else -1
    except asyncio.TimeoutError:
        timed_out = True
        proc.kill()
        stdout_b, stderr_b = await proc.communicate()
        exit_code = -9
    return (
        stdout_b.decode("utf-8", "replace"),
        stderr_b.decode("utf-8", "replace"),
        exit_code,
        timed_out,
    )


# ---------- session parsing ----------


def parse_claude_session(stdout: str) -> dict[str, Any]:
    """Extract the final JSON summary line of `claude -p --output-format json`.

    Args:
        stdout: The captured stdout from a claude run.

    Returns:
        A dict of session metrics (turns, tool_calls, session_id, and token
        counts), with values None when the summary line is absent or unparsable.
    """
    json_line = ""
    for line in reversed(stdout.splitlines()):
        t = line.lstrip()
        if t.startswith("{") or t.startswith("["):
            json_line = line
            break
    try:
        v = json.loads(json_line)
    except (json.JSONDecodeError, ValueError):
        v = {}
    usage = v.get("usage") or {}
    return {
        "turns": v.get("num_turns"),
        "tool_calls": v.get("num_tool_calls"),
        "session_id": v.get("session_id"),
        "input_tokens": usage.get("input_tokens"),
        "output_tokens": usage.get("output_tokens"),
        "total_tokens": usage.get("total_tokens"),
        "cache_read_tokens": usage.get("cache_read_input_tokens"),
        "cache_write_tokens": usage.get("cache_creation_input_tokens"),
    }


def session_log_path(staging: Path, session_id: str) -> Optional[Path]:
    """Compute the path to claude's session jsonl log for a staging dir.

    Args:
        staging: The staging directory the session ran in.
        session_id: The claude session id whose log file is sought.

    Returns:
        The expected jsonl log path under ~/.claude/projects, or None when HOME
        is unset.
    """
    home = os.environ.get("HOME")
    if not home:
        return None
    encoded = str(staging).replace("/", "-")
    return Path(home) / ".claude" / "projects" / encoded / f"{session_id}.jsonl"


def _preview(s: str, is_error: bool = False) -> str:
    """Truncate a string to a preview, keeping head and tail for errors.

    Non-error text is clipped to PREVIEW_CHARS; error text gets the larger
    ERROR_PREVIEW_CHARS budget split between head and tail with an elision marker.

    Args:
        s: The string to preview.
        is_error: Whether to use the larger error budget and head/tail split.

    Returns:
        The truncated preview string.
    """
    if not is_error:
        return s[:PREVIEW_CHARS]
    if len(s) <= ERROR_PREVIEW_CHARS:
        return s
    head, tail = s[:400], s[-(ERROR_PREVIEW_CHARS - 400):]
    return f"{head}\n…[truncated]…\n{tail}"


def _result_text(c: dict[str, Any]) -> str:
    """Extract the text payload from a tool_result content block.

    Args:
        c: A tool_result content block whose "content" is a string or a list of
            text parts.

    Returns:
        The flattened text, joining list parts with newlines; empty when absent.
    """
    content = c.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join(
            p.get("text", "") for p in content if isinstance(p, dict) and "text" in p
        )
    return ""


def parse_turn_log(jsonl: str) -> tuple[list[dict[str, Any]], int]:
    """Parse claude's session jsonl into per-assistant-turn structured rows.

    Returns (turn_log, api_calls). Each turn carries a 1-based `i`, an
    optional thinking/text preview, and a `tools` list whose result
    previews are back-filled from later tool_result user messages.

    Args:
        jsonl: The raw newline-delimited JSON contents of the session log.

    Returns:
        A (turn_log, api_calls) tuple where turn_log is the list of per-turn rows
        and api_calls is the count of assistant messages.
    """
    turns: list[dict[str, Any]] = []
    pending: dict[str, tuple[int, int]] = {}
    api_calls = 0

    for line in jsonl.splitlines():
        try:
            v = json.loads(line)
        except (json.JSONDecodeError, ValueError):
            continue
        typ = v.get("type")
        if typ == "assistant":
            api_calls += 1
            content = (v.get("message") or {}).get("content")
            if not isinstance(content, list):
                continue
            turn: dict[str, Any] = {"i": len(turns) + 1}
            thinking, text, tools = "", "", []
            for c in content:
                if not isinstance(c, dict):
                    continue
                ctype = c.get("type")
                if ctype == "thinking":
                    thinking += c.get("thinking", "") or ""
                elif ctype == "text":
                    text += c.get("text", "") or ""
                elif ctype == "tool_use":
                    tu = {
                        "name": c.get("name"),
                        "input": c.get("input"),
                        "result_preview": None,
                        "result_chars": 0,
                        "is_error": False,
                    }
                    tool_idx = len(tools)
                    tools.append(tu)
                    tid = c.get("id")
                    if isinstance(tid, str):
                        pending[tid] = (len(turns), tool_idx)
            if thinking:
                turn["thinking_chars"] = len(thinking)
                turn["thinking_preview"] = _preview(thinking)
            if text:
                turn["text_chars"] = len(text)
                turn["text_preview"] = _preview(text)
            turn["tools"] = tools
            turns.append(turn)
        elif typ == "user":
            content = (v.get("message") or {}).get("content")
            if not isinstance(content, list):
                continue
            for c in content:
                if not isinstance(c, dict) or c.get("type") != "tool_result":
                    continue
                tid = c.get("tool_use_id")
                if not isinstance(tid, str) or tid not in pending:
                    continue
                rtext = _result_text(c)
                ti, ix = pending.pop(tid)
                tool = turns[ti]["tools"][ix]
                is_err = bool(c.get("is_error", False))
                tool["result_chars"] = len(rtext)
                tool["result_preview"] = _preview(rtext, is_err)
                tool["is_error"] = is_err

    return turns, api_calls


def compute_cost(session: dict[str, Any], prices: dict[str, float]) -> float:
    """Compute the USD cost of a session from its token counts and prices.

    Args:
        session: The parsed session dict carrying input/output/cache token counts.
        prices: Per-million-token USD rates keyed by token category.

    Returns:
        The total estimated cost in USD.
    """
    def g(k: str) -> float:
        """Read a session token count as a float, treating missing/None as 0.

        Args:
            k: The session key to read.

        Returns:
            The token count as a float, or 0.0 when absent or None.
        """
        return float(session.get(k) or 0)

    return (
        g("input_tokens") * prices.get("input_per_million_usd", 0.0)
        + g("output_tokens") * prices.get("output_per_million_usd", 0.0)
        + g("cache_read_tokens") * prices.get("cache_read_per_million_usd", 0.0)
        + g("cache_write_tokens") * prices.get("cache_write_per_million_usd", 0.0)
    ) / 1_000_000.0


def host_metadata() -> dict[str, Any]:
    """Capture OS, architecture, timestamp, and optional hostname of the host.

    Returns:
        A dict with os, arch, captured_at, and hostname (when HOSTNAME is set).
    """
    m = {
        "os": platform.system().lower(),
        "arch": platform.machine(),
        "captured_at": datetime.now(timezone.utc).isoformat(),
    }
    if h := os.environ.get("HOSTNAME"):
        m["hostname"] = h
    return m


# ---------- post-file collection ----------


def collect_post_files(
    staging: Path, patterns: list[str], max_file_bytes: int, max_total_bytes: int
) -> dict[str, str]:
    """Collect text files under staging matching glob patterns within size caps.

    Walks the staging tree, including files whose relative path or name matches
    any pattern, skipping ones over the per-file cap and stopping once the total
    byte budget would be exceeded.

    Args:
        staging: The staging directory to walk.
        patterns: fnmatch glob patterns matched against each file's relative path
            or basename.
        max_file_bytes: Maximum size of any single collected file.
        max_total_bytes: Maximum cumulative size across all collected files.

    Returns:
        A mapping of relative path to file content for the collected files.
    """
    import fnmatch

    out: dict[str, str] = {}
    total = 0
    for abs_path in staging.rglob("*"):
        if not abs_path.is_file():
            continue
        try:
            rel = str(abs_path.relative_to(staging))
        except ValueError:
            continue
        if not any(fnmatch.fnmatch(rel, pat) or fnmatch.fnmatch(abs_path.name, pat)
                   for pat in patterns):
            continue
        size = abs_path.stat().st_size
        if size > max_file_bytes:
            continue
        if total + size > max_total_bytes:
            break
        try:
            out[rel] = abs_path.read_text()
            total += size
        except (UnicodeDecodeError, OSError):
            continue
    return out
