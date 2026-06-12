"""The draft/critique generation loop (ported from baml-changelog2 app/main.py).

All model calls go through claude-proxy — the same path every other worker in
this codebase uses (and billed via the proxy's Claude session, not raw API
credits). For each entry: collect the GitHub context (release, same-channel
predecessor, commit log, real file diff), then loop DRAFT (agent writes
entry.json) -> run every fenced code block with the release-matched baml-cli
-> CRITIQUE (agent writes critique.json), up to MAX_ATTEMPTS, and return the
final entry + how it was produced.
"""

from __future__ import annotations

import json
import logging
import os
import uuid
from typing import Any, Optional

import anyio

from bench_core import baml_cli_fetch, changelog_github, changelog_runner
from bench_core.jsonl import extract_last_json_object
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import RunAgentRequest

from .prompts import REVISE_ADDENDUM, SYSTEM_PROMPT_CRITIQUE, SYSTEM_PROMPT_DRAFT

log = logging.getLogger("changelog_worker")

# Opus is materially better at grounding the entry in the diff and at catching
# its own hallucinations during the critique step.
MODEL = os.environ.get("CHANGELOG_MODEL") or os.environ.get("CLAUDE_MODEL", "claude-opus-4-8")
# Reading a big release context chews turns: an ~80KB diff takes several
# chunked Read calls before the agent ever writes its output file, and a
# too-low cap kills the run mid-read (exit 1, transcript ending inside the
# diff). 16 covers the worst observed diff with room for the write.
AGENT_MAX_TURNS = int(os.environ.get("CHANGELOG_AGENT_MAX_TURNS", "16"))
AGENT_TIMEOUT_SECS = int(os.environ.get("CHANGELOG_AGENT_TIMEOUT_SECS", "600"))
# Total draft attempts (1 = no redraft; 3 = initial + up to 2 redrafts).
MAX_ATTEMPTS = int(os.environ.get("CHANGELOG_MAX_ATTEMPTS", "3"))

_SCORE_KEYS = ("grounding", "completeness", "specificity", "usefulness", "style", "runnable")


class GenerationError(RuntimeError):
    """Raised when an entry cannot be generated (GitHub, agent, or shape)."""


def _build_user_content(ctx: dict[str, Any]) -> str:
    parts = [
        f"Repository: {changelog_github.REPO}",
        f"Release: {ctx['version']}",
        f"Channel: {ctx.get('channel') or 'unknown'}",
        f"Date: {ctx['date'] or 'unknown'}",
    ]
    if ctx.get("from_version"):
        parts.append(
            f"Previous release on the same channel "
            f"({ctx.get('channel') or 'unknown'}): {ctx['from_version']}"
        )
        parts.append(
            "All diffs and commits below are EXACTLY the changes between "
            f"{ctx['from_version']} and {ctx['version']}."
        )
    if ctx.get("authors"):
        parts.append(f"Contributors: {', '.join(ctx['authors'])}")
    parts += [
        "",
        "Commit log for the range (oldest first):",
        ctx["commit_log"].strip() or "(no commits in the range)",
    ]
    if ctx.get("diff"):
        parts += [
            "",
            "File-level diff between the previous release and this one:",
            ctx["diff"],
        ]
    else:
        parts += [
            "",
            "(No file-level diff is available. Probably the very first "
            "release on this channel.)",
        ]
    return "\n".join(parts)


async def _run_json_agent(
    proxy: ProxyClient,
    *,
    system_prompt: str,
    files: dict[str, str],
    prompt: str,
    out_file: str,
) -> dict[str, Any]:
    """Run one proxy agent that must write a JSON object to `out_file`.

    Args:
        proxy: The claude-proxy client.
        system_prompt: The agent's system prompt.
        files: Files staged into the agent's working directory.
        prompt: The user prompt.
        out_file: The JSON file the agent writes (collected via post_files).

    Returns:
        The parsed JSON object (from the posted file, falling back to the
        last JSON object in the transcript).

    Raises:
        GenerationError: When the run errors or produces no parseable object.
    """
    req = RunAgentRequest(
        cell_id=f"changelog-{uuid.uuid4().hex[:10]}",
        model=MODEL,
        max_turns=AGENT_MAX_TURNS,
        prompt=prompt,
        system_prompt=system_prompt,
        files=files,
        post_file_patterns=[out_file],
        invocation_timeout_secs=AGENT_TIMEOUT_SECS,
    )
    result = await proxy.run_agent(req, timeout=AGENT_TIMEOUT_SECS + 120)
    if result.status != "ok":
        # stderr is often empty on CLI failures; the transcript tail usually
        # carries the actual error (rate limit, OOM kill, login issue).
        tail = (result.stderr_tail or "").strip()[-400:]
        ttail = (result.transcript or "").strip()[-400:]
        detail = tail or ttail or "(no stderr or transcript)"
        raise GenerationError(f"agent run {result.status} (exit {result.exit_code}): {detail}")
    raw = result.post_files.get(out_file)
    if raw:
        try:
            data = json.loads(raw)
            if isinstance(data, dict):
                return data
        except json.JSONDecodeError:
            pass
    scraped = extract_last_json_object(result.transcript or "")
    if isinstance(scraped, dict):
        return scraped
    raise GenerationError(f"agent did not produce a parseable {out_file}")


async def _draft(proxy: ProxyClient, ctx: dict[str, Any],
                 prior_critique: Optional[dict[str, Any]] = None,
                 revise_seed: Optional[dict[str, Any]] = None) -> dict[str, Any]:
    user = "Read context.md for the full release context, then write entry.json."
    addenda: list[str] = []
    # A revise seed (current entry + human guidance) only applies to the FIRST
    # attempt. On later attempts the prior critique drives the redraft, exactly
    # as in a fresh generation, so the critic still polices the revised text.
    if revise_seed and not prior_critique:
        addenda.append(REVISE_ADDENDUM.format(
            current_entry=json.dumps(revise_seed["current_entry"], indent=2),
            guidance=revise_seed["guidance"],
        ))
    if prior_critique:
        lines = [
            "---",
            "REDRAFT NEEDED. A quality reviewer scored the previous draft as:",
        ]
        lines += [f"  {k}={prior_critique.get(k)}" for k in _SCORE_KEYS]
        issues = prior_critique.get("issues") or []
        if issues:
            lines += ["", "Specific issues to fix:"] + [f"- {i}" for i in issues]
        hints = prior_critique.get("rewrite_hints") or ""
        if hints:
            lines += ["", f"Rewrite hints: {hints}"]
        lines += ["", "Apply the above and write a CORRECTED entry.json now."]
        addenda.append("\n".join(lines))
    if addenda:
        user += "\n\n" + "\n\n".join(addenda)
    return await _run_json_agent(
        proxy,
        system_prompt=SYSTEM_PROMPT_DRAFT,
        files={"context.md": _build_user_content(ctx)},
        prompt=user,
        out_file="entry.json",
    )


async def _critique(proxy: ProxyClient, ctx: dict[str, Any], draft: dict[str, Any],
                    code_report: changelog_runner.CodeCheckReport) -> dict[str, Any]:
    user = (
        "Read context.md (the release context the drafter saw), draft.json "
        "(the draft to review), and codecheck.txt (the authoritative results "
        "of actually running the draft's code blocks). Score each dimension "
        "and write critique.json."
    )
    critique = await _run_json_agent(
        proxy,
        system_prompt=SYSTEM_PROMPT_CRITIQUE,
        files={
            "context.md": _build_user_content(ctx),
            "draft.json": json.dumps(draft, indent=2),
            "codecheck.txt": code_report.summary_for_prompt(),
        },
        prompt=user,
        out_file="critique.json",
    )
    critique.setdefault("verdict", "revise")
    critique.setdefault("issues", [])
    critique.setdefault("rewrite_hints", "")
    return critique


async def _draft_and_critique_loop(
    proxy: ProxyClient, ctx: dict[str, Any],
    revise_seed: Optional[dict[str, Any]] = None,
    baml_cli: Optional[str] = None,
) -> tuple[dict[str, Any], dict[str, Any], int, changelog_runner.CodeCheckReport]:
    """Run draft↔critique up to MAX_ATTEMPTS. Returns (final_entry,
    final_critique, attempts_used, final_code_report).

    When `revise_seed` is given the first draft starts from an existing entry +
    human guidance instead of from scratch; the critique loop is otherwise
    identical. `baml_cli` is the release-matched CLI used to actually run the
    BAML snippets (None falls back to the image's baml-cli)."""
    entry: dict[str, Any] = {}
    critique: dict[str, Any] = {}
    code_report = changelog_runner.CodeCheckReport()
    for attempt in range(1, MAX_ATTEMPTS + 1):
        prior = critique if attempt > 1 else None
        entry = await _draft(proxy, ctx, prior_critique=prior, revise_seed=revise_seed)

        # Actually run the code blocks in the draft. This is the authoritative
        # `runnable` dimension: the critique agent cannot be trusted to execute
        # code, so the harness does it (with the release-matched CLI) and the
        # result is non-negotiable. Blocking work runs off the event loop.
        body = entry.get("body") or ""
        code_report = await anyio.to_thread.run_sync(
            lambda: changelog_runner.check_body(body, baml_cli=baml_cli)
        )
        critique = await _critique(proxy, ctx, entry, code_report)
        critique["runnable"] = code_report.runnable_score()
        if code_report.failed:
            # A broken code block blocks approval regardless of how the agent
            # scored the prose. Feed the exact errors to the next redraft.
            critique["verdict"] = "revise"
            critique["issues"] = list(critique.get("issues") or []) + code_report.issues()
            existing = (critique.get("rewrite_hints") or "").strip()
            critique["rewrite_hints"] = (
                existing + "\n\n" + code_report.rewrite_hint()
            ).strip() if existing else code_report.rewrite_hint()

        log.info(
            "release=%s attempt=%d verdict=%s scores=%s code=%s",
            ctx.get("version"),
            attempt,
            critique.get("verdict"),
            {k: critique.get(k) for k in _SCORE_KEYS},
            {r.lang + "#" + str(r.index): r.status for r in code_report.results} or "(no code)",
        )
        if critique.get("verdict") == "approve":
            return entry, critique, attempt, code_report
    # Exhausted retries. Ship the last draft anyway; the row's meta surfaces the
    # unresolved critique so a human can requeue if they want another pass.
    return entry, critique, MAX_ATTEMPTS, code_report


async def generate(proxy: ProxyClient, tag: str, from_release: Optional[str] = None,
                   revise_seed: Optional[dict[str, Any]] = None) -> dict[str, Any]:
    """Generate (or revise) the entry for a release tag via claude-proxy.

    Args:
        proxy: The claude-proxy client used for the draft/critique agents.
        tag: The GitHub release tag (e.g. ``baml-language-0.222.0``).
        from_release: Optional predecessor tag overriding the channel default.
        revise_seed: Optional ``{current_entry, guidance}`` for a revise pass.

    Returns:
        A dict with the entry fields (version, date, title, body, authors,
        channel) plus ``meta`` (compared_against, attempts, scores,
        final_verdict, code_checks).

    Raises:
        GenerationError: When the context, agent run, or entry shape fails.
    """
    try:
        ctx = await anyio.to_thread.run_sync(
            lambda: changelog_github.collect_context(tag, from_release)
        )
    except changelog_github.GitHubError as e:
        raise GenerationError(f"github: {e}") from e

    log.info(
        "release=%s channel=%s compared_against=%s diff_chars=%d",
        ctx.get("version"), ctx.get("channel"), ctx.get("from_version"),
        len(ctx.get("diff") or ""),
    )

    # Validate the entry's code blocks with the baml-cli built from THIS
    # release, so a "pass" means it works in the version being documented.
    pinned = await anyio.to_thread.run_sync(lambda: baml_cli_fetch.resolve_cli(tag))

    entry, critique, attempts, code_report = await _draft_and_critique_loop(
        proxy, ctx, revise_seed=revise_seed, baml_cli=pinned
    )

    # Trust GitHub for the deterministic fields.
    entry["version"] = ctx["version"]
    entry["channel"] = ctx.get("channel") or "unknown"
    if ctx["date"]:
        entry["date"] = ctx["date"]
    elif not entry.get("date"):
        raise GenerationError("no date available")
    if not entry.get("title") or not entry.get("body"):
        raise GenerationError("agent returned an empty title or body")

    entry["meta"] = {
        "compared_against": ctx.get("from_version"),
        "channel": ctx.get("channel"),
        "attempts": attempts,
        "scores": {k: critique.get(k) for k in _SCORE_KEYS},
        "final_verdict": critique.get("verdict"),
        "code_checks": [
            {"block": r.index, "lang": r.lang, "status": r.status,
             "error": r.error or None}
            for r in code_report.results
        ],
    }
    return entry
