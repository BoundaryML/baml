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
import signal
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

# The pure transcript parsers live in bench_core so the API can parse a local
# `.jsonl` upload identically to a live proxy run. Re-exported here (imported
# into this module's namespace) so existing ``runner.parse_*`` callers keep working.
from bench_core.transcript import (  # noqa: F401
    _preview,
    _result_text,
    compute_cost,
    parse_claude_session,
    parse_turn_log,
    render_terminal_transcript,
)


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


def _reap_process_group(proc: "asyncio.subprocess.Process") -> None:
    """SIGKILL the whole process group led by ``proc``.

    The agent (and the proxy's own ``baml`` commands) can spawn grandchildren —
    e.g. a ``baml test`` call blocked on a ``baml.net.TcpStream`` read with no
    timeout. Those grandchildren inherit the child's stdout/stderr pipes, so a
    plain ``proc.kill()`` (which only kills the direct child) leaves them alive
    holding the pipes open: the proxy's pipe read never sees EOF and the whole
    run wedges indefinitely — even past the wall-clock timeout. We spawn the
    child as a process-group leader (``start_new_session=True``), so its pid is
    the group id; killing the group reaps every descendant.

    Args:
        proc: The child process spawned with ``start_new_session=True``.
    """
    try:
        os.killpg(proc.pid, signal.SIGKILL)  # pid == pgid for a session leader
    except (ProcessLookupError, PermissionError):
        pass  # group already gone (clean exit) or not ours — nothing to reap


async def _drain(stream: "Optional[asyncio.StreamReader]", sink: list[bytes]) -> None:
    """Read a pipe to EOF into ``sink``.

    Draining concurrently with the wait below keeps the child from blocking on a
    full stdout pipe, and lets us collect output even when we have to reap a
    hung process group to force EOF.

    Args:
        stream: The stdout/stderr reader, or None.
        sink: List that receives the read chunks.
    """
    if stream is None:
        return
    while True:
        chunk = await stream.read(65536)
        if not chunk:
            break
        sink.append(chunk)


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
    else:
        # OAuth mode: an inherited key (e.g. Infisical-injected) would silently
        # shadow the CLI's persisted subscription login — scrub it.
        env.pop("ANTHROPIC_API_KEY", None)
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
        start_new_session=True,  # own process group so we can reap grandchildren
    )

    out_chunks: list[bytes] = []
    err_chunks: list[bytes] = []
    drain_out = asyncio.create_task(_drain(proc.stdout, out_chunks))
    drain_err = asyncio.create_task(_drain(proc.stderr, err_chunks))

    async def _feed_and_wait() -> None:
        """Feed the prompt on stdin, then wait for claude *itself* to exit.

        We wait on ``proc.wait()`` (the claude process), not on pipe EOF, so a
        leaked grandchild holding the pipes can't keep us blocked here.
        """
        if proc.stdin is not None:
            try:
                proc.stdin.write(prompt.encode())
                await proc.stdin.drain()
            except (BrokenPipeError, ConnectionResetError):
                pass
            finally:
                proc.stdin.close()
        await proc.wait()

    timed_out = False
    try:
        await asyncio.wait_for(_feed_and_wait(), timeout=timeout_secs)
        exit_code = proc.returncode if proc.returncode is not None else -1
    except asyncio.TimeoutError:
        timed_out = True
        exit_code = -9

    # Reap claude AND any leaked grandchildren (e.g. a `baml` call stuck on a
    # socket read) so they can't hold the pipes open and wedge the run. Then the
    # drains see EOF; bound the final read so a pathological case still returns.
    _reap_process_group(proc)
    try:
        await asyncio.wait_for(asyncio.gather(drain_out, drain_err), timeout=15)
    except asyncio.TimeoutError:
        drain_out.cancel()
        drain_err.cancel()

    stdout = b"".join(out_chunks).decode("utf-8", "replace")
    stderr = b"".join(err_chunks).decode("utf-8", "replace")
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
        start_new_session=True,  # own process group so we can reap grandchildren
    )
    out_chunks: list[bytes] = []
    err_chunks: list[bytes] = []
    drain_out = asyncio.create_task(_drain(proc.stdout, out_chunks))
    drain_err = asyncio.create_task(_drain(proc.stderr, err_chunks))
    timed_out = False
    try:
        await asyncio.wait_for(proc.wait(), timeout=timeout_secs)
        exit_code = proc.returncode if proc.returncode is not None else -1
    except asyncio.TimeoutError:
        timed_out = True
        exit_code = -9
    # Reap the shell AND any grandchildren it spawned (e.g. a hung `baml`) so a
    # leaked child can't hold the pipes open past the timeout.
    _reap_process_group(proc)
    try:
        await asyncio.wait_for(asyncio.gather(drain_out, drain_err), timeout=15)
    except asyncio.TimeoutError:
        drain_out.cancel()
        drain_err.cancel()
    return (
        b"".join(out_chunks).decode("utf-8", "replace"),
        b"".join(err_chunks).decode("utf-8", "replace"),
        exit_code,
        timed_out,
    )


# ---------- session parsing ----------
# parse_claude_session / parse_turn_log / compute_cost / _preview / _result_text
# now live in bench_core.transcript and are re-exported via the import above.


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
    byte budget would be exceeded. The .claude/ and .agents/ trees are excluded:
    they hold installed agent config (e.g. the skills `baml agent install`
    writes), not artifacts the agent authored, and their SKILL.md files would
    otherwise burn the byte budget before real artifacts are collected.

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
        if rel.startswith((".claude/", ".agents/")):
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
