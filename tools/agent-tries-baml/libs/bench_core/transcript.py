"""Pure parsers for Claude Code session transcripts.

Extracted verbatim from claude-proxy's runner so both the proxy (live runs) and
the API (local-run uploads) parse a `.jsonl` session identically. No I/O, no
third-party deps -- just the session-summary line, the per-turn structured log,
preview truncation, and cost arithmetic.

The 1-based turn `i` index produced here is what the worker cites as
``call_index``, so it must match byte-for-byte across producers.
"""

from __future__ import annotations

import json
from typing import Any

PREVIEW_CHARS = 400
# Error results get a larger budget: baml prints the real `error:` line after a
# stack of "Loading …baml" lines, so 400 head-chars often cuts it off. Keep both
# the head (the command echo) and the tail (where the error lands).
ERROR_PREVIEW_CHARS = 2000


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


def _tool_call_summary(name: str, inp: Any) -> str:
    """Render a tool call's input as a concise one-line argument, terminal-style.

    Args:
        name: The tool name (e.g. ``Bash``, ``Read``).
        inp: The tool's input object.

    Returns:
        A short argument string for the ``Name(arg)`` header.
    """
    if not isinstance(inp, dict):
        return "" if inp is None else str(inp)
    if name == "Bash":
        return str(inp.get("command", ""))
    if name in ("Read", "Write", "Edit", "MultiEdit", "NotebookEdit"):
        return str(inp.get("file_path") or inp.get("notebook_path") or "")
    if name in ("Glob", "Grep"):
        return str(inp.get("pattern", ""))
    return json.dumps(inp, separators=(",", ":"))


def render_terminal_transcript(jsonl: str) -> str:
    """Render a Claude Code session jsonl into a terminal-style transcript.

    Mimics the Claude Code CLI: user prompts marked ``>``, assistant text and
    tool calls marked ``⏺``, tool results marked ``⎿``, and thinking under a
    ``✻ Thinking…`` header. Untruncated, so the raw view stays searchable.

    Args:
        jsonl: The raw newline-delimited JSON contents of the session log.

    Returns:
        A plain-text transcript that reads like a Claude Code terminal session.
    """
    out: list[str] = []

    def block(text: str, marker: str, cont: str) -> None:
        """Append a marked first line plus indented continuation lines."""
        parts = (text.rstrip("\n") or "").split("\n")
        out.append(f"{marker}{parts[0]}")
        out.extend(f"{cont}{p}" for p in parts[1:])
        out.append("")

    for line in jsonl.splitlines():
        try:
            v = json.loads(line)
        except (json.JSONDecodeError, ValueError):
            continue
        typ = v.get("type")
        content = (v.get("message") or {}).get("content")
        if typ == "user":
            if isinstance(content, str):
                block(content, "> ", "  ")
            elif isinstance(content, list):
                for c in content:
                    if not isinstance(c, dict):
                        continue
                    if c.get("type") == "text":
                        block(c.get("text", ""), "> ", "  ")
                    elif c.get("type") == "tool_result":
                        body = _result_text(c).strip()
                        if c.get("is_error"):
                            body = f"[error] {body}"
                        block(body or "(no output)", "  ⎿  ", "     ")
        elif typ == "assistant" and isinstance(content, list):
            for c in content:
                if not isinstance(c, dict):
                    continue
                ctype = c.get("type")
                if ctype == "thinking" and (c.get("thinking") or "").strip():
                    block(c["thinking"].strip(), "✻ Thinking…\n", "  ")
                elif ctype == "text" and (c.get("text") or "").strip():
                    block(c["text"].strip(), "⏺ ", "  ")
                elif ctype == "tool_use":
                    name = c.get("name") or "tool"
                    out.append(f"⏺ {name}({_tool_call_summary(name, c.get('input'))})")
    return "\n".join(out).rstrip() + "\n"


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
