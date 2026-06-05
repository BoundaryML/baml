"""Render trophies -> reports.md and open issues -> open_issues.json
(ported from benchmark-builder/src/dedup.rs render_* helpers)."""

from __future__ import annotations

import json
from typing import Any


def render_reports_md(trophies: list[dict[str, Any]]) -> str:
    """Render a batch of trophies into the reports.md document fed to the agent.

    Emits each trophy's outcome, summary, verbose report narrative, candidate
    findings, and suggested improvements under a per-report header.

    Args:
        trophies: The trophy rows to render.

    Returns:
        The assembled reports.md markdown as a single string.
    """
    out = ["# Reports to dedup", ""]
    for t in trophies:
        out.append(f"--- report_id: {t['_id']} ---")
        out.append(f"Run outcome: {t.get('outcome')}")
        if t.get("summary"):
            out.append(f"Summary: {t['summary']}")
        # Full verbose narrative the agent wrote (the trophy is the report).
        report_md = t.get("reportMd")
        if report_md:
            out.append("")
            out.append(report_md.strip())
        findings = t.get("findings") or []
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
        suggestions = t.get("suggestions") or []
        if suggestions:
            out.append("")
            out.append("## Suggested improvements")
            for sug in suggestions:
                out.append(
                    f"- [{sug.get('target', '')}] {sug.get('suggestion', '')}"
                    f" ({sug.get('rationale', '')})"
                )
        out.append("")
        out.append("")
    return "\n".join(out)


def render_open_issues_json(issues: list[dict[str, Any]]) -> str:
    """Render open issues into the open_issues.json document fed to the agent.

    Projects each issue down to the id, kind, title, and description fields
    the agent needs for deduplication.

    Args:
        issues: The open issue rows to render.

    Returns:
        The pretty-printed JSON array as a string.
    """
    arr = [
        {"id": i["_id"], "kind": i["kind"], "title": i["title"], "description": i["description"]}
        for i in issues
    ]
    return json.dumps(arr, indent=2)
