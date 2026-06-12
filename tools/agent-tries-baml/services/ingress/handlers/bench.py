"""Bench-task creation: directive parsing + task/cohort fan-out.

Moved out of services/ingress/app.py so the @bammy router can dispatch here
without a circular import. The directive grammar is unchanged: `[baml=N]`,
`[canary]`/`[nightly]`, `[coldstart]`/`[cold]`, `[skill arena(: branches)]`.
"""

from __future__ import annotations

import logging
import os
import re
from typing import Any, Optional

from bench_core.service_client import ServiceClient

log = logging.getLogger("uvicorn.error")

# Per-run mode directives, matched anywhere in the message and stripped from the
# prompt: `[baml=N]` pins the Nth-newest ready build (newest=1); `[canary]`/
# `[nightly]` select the channel (default nightly); `[coldstart]` (alias `[cold]`)
# runs with no prebuilt baml so the agent installs it itself.
_DIRECTIVE_BAML = re.compile(r"\[\s*baml\s*=\s*([^\]]+?)\s*\]", re.IGNORECASE)
_DIRECTIVE_COLD = re.compile(r"\[\s*cold(?:start)?\s*\]", re.IGNORECASE)
_DIRECTIVE_CHANNEL = re.compile(r"\[\s*(canary|nightly)\s*\]", re.IGNORECASE)
# `[skill arena]` runs the same task once per baml-skill branch and compares the
# outcomes (a "cohort"). The branches default to ATB_ARENA_BRANCHES, or are given
# inline as a comma list: `[skill arena: main, exp-a, exp-b]`.
_DIRECTIVE_ARENA = re.compile(r"\[\s*skill\s+arena\s*(?::\s*([^\]]+?))?\s*\]", re.IGNORECASE)
# Default skill-arena branches when the directive names none (comma-separated).
ARENA_BRANCHES_DEFAULT = os.environ.get("ATB_ARENA_BRANCHES", "main")


def has_directives(text: str) -> bool:
    """Report whether the text carries any explicit per-run directive.

    The @bammy router uses this as its zero-model fast path: a directive is an
    unambiguous bench opt-in, so the intent classifier never runs on it.

    Args:
        text: The user's message (bot mention already stripped).

    Returns:
        True when any directive pattern matches.
    """
    return bool(
        _DIRECTIVE_BAML.search(text)
        or _DIRECTIVE_COLD.search(text)
        or _DIRECTIVE_CHANNEL.search(text)
        or _DIRECTIVE_ARENA.search(text)
    )


def _parse_branches(csv: str) -> list[str]:
    """Split a comma-separated branch list into a clean, de-duplicated list.

    Args:
        csv: Comma-separated branch names (inline directive value or env default).

    Returns:
        Branch names in order, trimmed, with blanks and duplicates removed.
    """
    out: list[str] = []
    for raw in (csv or "").split(","):
        ref = raw.strip()
        if ref and ref not in out:
            out.append(ref)
    return out


def _parse_run_directives(text: str) -> tuple[str, dict[str, Any]]:
    """Extract per-run mode directives from a prompt and strip them out.

    Recognizes ``[baml=N]`` (a build selector, kept as a raw string the worker
    resolves at run time), ``[canary]``/``[nightly]`` (channel, default nightly),
    and ``[coldstart]``/``[cold]`` (cold-start mode). Cold start takes precedence
    over a pin — both withhold the channel's latest binary — so a pin alongside
    cold start is ignored (the channel is still recorded but unused for cold runs).

    Args:
        text: The user's message (bot mention already stripped).

    Returns:
        A tuple of (cleaned_prompt, options) where options may contain
        ``coldStart: True``, ``bamlPin: "<selector>"``, and/or
        ``bamlChannel: "nightly"|"canary"``.
    """
    opts: dict[str, Any] = {}
    cold = bool(_DIRECTIVE_COLD.search(text))
    m = _DIRECTIVE_BAML.search(text)
    ch = _DIRECTIVE_CHANNEL.search(text)
    arena = _DIRECTIVE_ARENA.search(text)
    cleaned = _DIRECTIVE_ARENA.sub(
        "", _DIRECTIVE_CHANNEL.sub("", _DIRECTIVE_COLD.sub("", _DIRECTIVE_BAML.sub("", text)))
    )
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    if ch:
        opts["bamlChannel"] = ch.group(1).lower()
    if cold:
        opts["coldStart"] = True
        if m:
            log.info("ingress: both coldstart and baml pin given; cold start wins")
    elif m:
        opts["bamlPin"] = m.group(1).strip()
    if arena:
        # arenaBranches is consumed by the caller to fan out a cohort, not stored on
        # a task. The remaining opts (pin/channel/coldStart) still apply to each
        # member run, so the arena can vary the skill while pinning baml.
        branches = _parse_branches(arena.group(1) or ARENA_BRANCHES_DEFAULT)
        if branches:
            opts["arenaBranches"] = branches
    return cleaned, opts


async def _create_cohort(service: ServiceClient, prompt: str, branches: list[str],
                         base_opts: dict[str, Any], *, source: str,
                         slack: Optional[dict[str, Any]] = None) -> str:
    """Fan out a skill-arena cohort: one cohort row plus one member task per branch.

    Creates the cohort ``pending`` (carrying the slack routing so CohortCompare can
    post the comparison), then one ``queued`` member task per branch — each tagged
    with the cohort id and its ``skillRef``, and inheriting the run's other
    directives (pin/channel/coldStart). Member tasks carry no slack routing so the
    workers stay quiet; the single comparison is posted by CohortCompare. Finally
    records the member ids on the cohort for the fan-in reconciler.

    Args:
        service: Service client used for the creates.
        prompt: The task prompt (directives already stripped).
        branches: The baml-skill branches to run, one member task each.
        base_opts: Per-run directives (bamlPin/bamlChannel/coldStart) applied to
            every member task.
        source: Origin tag for the cohort and its tasks (``slack``/``bug_report``).
        slack: Optional slack routing (channel/thread/user) recorded on the cohort.

    Returns:
        The created cohort's id.
    """
    cohort_doc: dict[str, Any] = {
        "prompt": prompt, "skillRefs": branches, "memberTaskIds": [],
        "source": source, "status": "pending",
    }
    if slack:
        cohort_doc.update({k: v for k, v in slack.items() if v is not None})
    cohort_id = await service.create("cohorts", cohort_doc)
    member_ids: list[str] = []
    for ref in branches:
        tid = await service.create("tasks", {
            "source": source, "prompt": prompt, "status": "queued",
            "cohortId": cohort_id, "skillRef": ref, **base_opts,
        })
        member_ids.append(tid)
    await service.update("cohorts", cohort_id, {"memberTaskIds": member_ids})
    return cohort_id


async def create_slack_task(service: ServiceClient, event: dict[str, Any], text: str,
                            eid: Optional[str], *, thread_context: str = "") -> None:
    """Create a Slack-sourced task (or skill-arena cohort) off the request path.

    Runs after we have already ACKed Slack, so a slow Convex write can never push
    us past Slack's 3s deadline. Failures are logged, not raised (the ACK is gone).
    A ``[skill arena]`` directive fans out a cohort instead of a single task.

    Args:
        service: Service client used for the creates.
        event: The Slack event object (channel, ts, thread_ts, user).
        text: The user's prompt, with a leading bot mention already stripped.
        eid: The Slack event id, for log correlation.
        thread_context: Rendered prior thread messages; appended to the prompt
            so a mid-thread bench request inherits the discussion.
    """
    try:
        prompt, opts = _parse_run_directives(text)
        if thread_context:
            prompt = f"{prompt}\n\nSlack thread context (messages before this request):\n{thread_context}"
        slack = {
            "slackChannel": event.get("channel"),
            "slackThreadTs": event.get("thread_ts") or event.get("ts"),
            "slackUser": event.get("user"),
        }
        branches = opts.pop("arenaBranches", None)
        if branches:
            cid = await _create_cohort(service, prompt, branches, opts, source="slack", slack=slack)
            log.info("slack: created cohort %s (%d variants) event_id=%s text=%r",
                     cid, len(branches), eid, prompt[:80])
            return
        tid = await service.create("tasks", {
            "source": "slack", "prompt": prompt, "status": "queued", **slack, **opts,
        })
        log.info("slack: created task %s event_id=%s opts=%s text=%r", tid, eid, opts, prompt[:80])
    except Exception:  # noqa: BLE001
        log.exception("slack: failed to create task for event_id=%s", eid)
