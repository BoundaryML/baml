"""Render a cohort's member runs into the arena.md document fed to the compare agent.

Mirrors baml_dedup/render.py: it flattens each variant's held trophy (outcome, metrics,
summary, what went well/failed, full report, candidate findings + verified repro) under a
per-variant header keyed by the skill branch, so the agent can compare like for like.
"""

from __future__ import annotations

from typing import Any, Optional


def _metrics_line(metrics: dict[str, Any]) -> str:
    """Render a one-line metrics summary for a variant.

    Args:
        metrics: The trophy's metric bag.

    Returns:
        A compact ``turns · api · cost`` line.
    """
    m = metrics or {}
    cost = m.get("estimated_cost_usd")
    cost_s = f"${cost}" if cost is not None else "$-"
    return (f"turns: {m.get('turns', '-')} · api_calls: {m.get('api_calls', '-')} · "
            f"tool_calls: {m.get('tool_calls', '-')} · cost: {cost_s}")


def render_arena_md(cohort: dict[str, Any],
                    variants: list[tuple[dict[str, Any], Optional[dict[str, Any]]]]) -> str:
    """Render a cohort and its member runs into the arena.md comparison document.

    Args:
        cohort: The cohort row (provides the shared prompt and the branch list).
        variants: Per-variant ``(member_task, trophy)`` pairs; ``trophy`` is None
            when a member produced none (e.g. it failed before reporting).

    Returns:
        The assembled arena.md markdown as a single string.
    """
    out = ["# Skill arena: one task, N skill versions", ""]
    out.append(f"Task prompt:\n{cohort.get('prompt', '')}")
    out.append("")
    out.append(f"{len(variants)} variant(s); branches compared: "
               f"{', '.join(cohort.get('skillRefs') or [])}")
    out.append("")
    for task, trophy in variants:
        ref = task.get("skillRef") or "(unknown branch)"
        if trophy is None:
            out.append(f"--- variant: {ref} ---")
            out.append(f"Member task {task['_id']} (status {task.get('status')}) produced no "
                       f"trophy — treat this variant as a failure/no-result.")
            out.append("")
            out.append("")
            continue
        out.append(f"--- variant: {ref} ---")
        out.append(f"report_id: {trophy['_id']}")
        out.append(f"Run outcome: {trophy.get('outcome')}")
        out.append(f"baml: {trophy.get('bamlVersion')}")
        out.append(_metrics_line(trophy.get("metrics") or {}))
        if trophy.get("summary"):
            out.append(f"Summary: {trophy['summary']}")
        if trophy.get("whatWentWell"):
            out.append("What went well:")
            out += [f"- {x}" for x in trophy["whatWentWell"]]
        if trophy.get("whatFailed"):
            out.append("What failed:")
            out += [f"- {x}" for x in trophy["whatFailed"]]
        if trophy.get("reportMd"):
            out.append("")
            out.append(trophy["reportMd"].strip())
        findings = trophy.get("findings") or []
        if findings:
            out.append("")
            out.append("Candidate findings:")
            for f in findings:
                anchor = f.get("anchor") or {}
                call = anchor.get("call_index")
                call_s = f" (call {call})" if call is not None else ""
                out.append(f"- [{f.get('kind')}] {f.get('title')}: {f.get('description')}{call_s}")
                if f.get("suggestion"):
                    out.append(f"  suggestion: {f['suggestion']}")
                if f.get("repro"):
                    out.append("\n## Minimal artifacts (verified)\n")
                    out.append(f["repro"])
        out.append("")
        out.append("")
    return "\n".join(out)
