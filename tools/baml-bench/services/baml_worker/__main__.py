"""BAML worker processor: claim a task, run the agent (with canary baml on PATH
and the BAML skill injected). The agent assembles the whole trophy itself as one
verbose `trophy.json` (report + issues + suggestions); the worker verifies its
repros, creates the trophy, and finishes the task. One agent per DB, no reviewer.
"""

from __future__ import annotations

import json
import logging
import os
from pathlib import Path
from typing import Any, Optional

from bench_core import slack_client
from bench_core.jsonl import extract_last_json_object
from bench_core.prices import prices_for
from bench_core.processor import Processor, run_processor
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import CheckBamlRequest, RunAgentRequest

from .prompts import WORKER_SYSTEM_PROMPT

log = logging.getLogger("baml_worker")

CLAUDE_MODEL = os.environ.get("CLAUDE_MODEL", "claude-sonnet-4-6")
CLAUDE_MAX_TURNS = int(os.environ.get("CLAUDE_MAX_TURNS", "40"))
AGENT_TIMEOUT_SECS = int(os.environ.get("WORKER_AGENT_TIMEOUT_SECS", "3600"))
SLACK_BOT_TOKEN = os.environ.get("SLACK_BOT_TOKEN", "")
UI_BASE_URL = os.environ.get("UI_BASE_URL", "https://bench3-ui.fly.dev")
BAML_SKILL_PATH = os.environ.get("BAML_SKILL_PATH", "")   # single combined file
BAML_SKILL_DIR = os.environ.get("BAML_SKILL_DIR", "")     # dir of */SKILL.md (preferred)
POST_FILE_PATTERNS = ["**/*.baml", "baml_src/**/*.baml", "*.md", "trophy.json"]

_SKILL_CACHE: Optional[str] = None
_SKILL_LOADED = False


def _load_skill() -> Optional[str]:
    """Load and cache the BAML skill text injected into the agent's workspace.

    Reads every SKILL.md under BAML_SKILL_DIR (preferred, concatenated) or the
    single BAML_SKILL_PATH file, caching the result so the disk read happens once
    per process.

    Returns:
        The combined skill markdown, or None when no skill source is configured.
    """
    global _SKILL_CACHE, _SKILL_LOADED
    if _SKILL_LOADED:
        return _SKILL_CACHE
    _SKILL_LOADED = True
    if BAML_SKILL_DIR and Path(BAML_SKILL_DIR).is_dir():
        parts = []
        for skill_md in sorted(Path(BAML_SKILL_DIR).rglob("SKILL.md")):
            rel = skill_md.parent.name
            parts.append(f"# BAML skill: {rel}\n\n{skill_md.read_text()}")
        _SKILL_CACHE = "\n\n---\n\n".join(parts) or None
    elif BAML_SKILL_PATH and Path(BAML_SKILL_PATH).exists():
        _SKILL_CACHE = Path(BAML_SKILL_PATH).read_text()
    return _SKILL_CACHE


def _derive_outcome(agent_status: Optional[str], task_completed: Any) -> str:
    """Map the agent's run status and self-report into a trophy outcome.

    Args:
        agent_status: The proxy-reported agent status (e.g. "ok", "timeout").
        task_completed: The agent's self-reported completion (True, False, or
            "partial").

    Returns:
        One of "success", "partial", or "failed".
    """
    # Outcome reflects whether the task was accomplished, NOT whether findings
    # exist. A clean success can still carry baml-friction findings.
    if agent_status == "timeout":
        return "failed"
    if agent_status not in ("ok", None):
        return "failed"
    if task_completed is False:
        return "failed"
    if task_completed == "partial":
        return "partial"
    return "success"


def _mine_baml_errors(turn_log: list[dict]) -> list[dict]:
    """Mine baml-attributable tool errors from the turn log as tentative findings.

    Deterministic backstop: every failed baml Bash command in the turn log becomes
    a tentative finding, so friction is never lost if the agent forgot to log it.
    The kind is tentative ("language"); dedup is the authoritative classifier.
    Excludes non-baml errors (e.g. a Write-before-Read tool error).

    Args:
        turn_log: The agent's per-turn log of tool calls and their results.

    Returns:
        A list of tentative finding dicts, one per baml command that errored.
    """
    out: list[dict] = []
    for turn in turn_log or []:
        i = turn.get("i")
        for tool in turn.get("tools") or []:
            if not tool.get("is_error"):
                continue
            if (tool.get("name") or "") != "Bash":
                continue
            cmd = ""
            inp = tool.get("input")
            if isinstance(inp, dict):
                cmd = str(inp.get("command") or "")
            elif isinstance(inp, str):
                cmd = inp
            if "baml" not in cmd.lower():
                continue  # only baml-attributable shell errors
            res = (tool.get("result_preview") or "").strip()
            out.append({
                "kind": "language",
                "title": f"`baml` command failed (call {i})"[:80],
                "description": f"`{cmd[:200]}` exited with an error.\n{res[:600]}".strip(),
                "anchor": {"call_index": i, "turn_index": i},
                # No runnable repro spec (we only have the command, not the project
                # state), so leave repro unset; _verify_repros won't fabricate one.
            })
    return out


class BamlWorker(Processor):
    """Claim a task, run the BAML agent, verify its repros, and create a trophy."""

    role = "baml-worker"
    table = "tasks"
    claim_value = "queued"
    claim_into = "running"
    lease_ms = 70 * 60 * 1000

    def __init__(self, service):
        """Bind the service client and build a proxy client from the environment.

        Args:
            service: Client used for all claim, transition, and persistence calls.
        """
        super().__init__(service)
        self.proxy = ProxyClient.from_env()

    async def process(self, item: dict[str, Any]) -> None:
        """Run one task end to end and persist its trophy.

        Acks the task on Slack, records the baml version, runs the agent through
        the proxy, stores the transcript, parses the agent's self-reported
        trophy.json, merges in mined baml errors, verifies each repro, derives the
        outcome, creates the trophy, transitions the task to done, and notifies.

        Args:
            item: The claimed task document.
        """
        task_id = item["_id"]
        # Slack "on it" ack at pickup time, linking to the task on the dashboard.
        if item.get("slackChannel"):
            link = f"{UI_BASE_URL.rstrip('/')}/tasks/{task_id}"
            await slack_client.post_message(
                SLACK_BOT_TOKEN,
                item["slackChannel"],
                f"_Running `{item['prompt'][:120]}` …_  <{link}|track on dashboard>",
                thread_ts=item.get("slackThreadTs"),
            )

        baml = await self.service.baml_current()
        baml_version = baml["sha"] if baml else None
        # Record the baml version on the task immediately so the in-flight task
        # page and the tasks table show which baml this run is using.
        if baml_version:
            await self.service.update(self.table, task_id, {"bamlVersion": baml_version})
        prices = prices_for(CLAUDE_MODEL)

        skill = _load_skill()
        files: dict[str, str] = {}
        if skill:
            files["SKILL.md"] = skill

        req = RunAgentRequest(
            cell_id=task_id,
            model=CLAUDE_MODEL,
            max_turns=CLAUDE_MAX_TURNS,
            prompt=item["prompt"],
            system_prompt=WORKER_SYSTEM_PROMPT,
            files=files,
            prices=prices,
            baml_version=baml_version,
            post_file_patterns=POST_FILE_PATTERNS,
            invocation_timeout_secs=AGENT_TIMEOUT_SECS,
        )
        log.info("running task %s (baml=%s)", task_id, baml_version)
        result = await self.proxy.run_agent(req)

        # Stash the full transcript as a blob (the trophy links it). put_transcript
        # creates the pointer *after* this task was claimed, so use its returned id
        # rather than the stale claim-time item.
        transcript_storage_id = item.get("transcriptStorageId")
        if result.transcript:
            transcript_storage_id = await self.service.put_transcript(
                self.table, task_id, result.transcript
            )
        turn_log = result.turn_log or []
        metrics = {
            "turns": result.turns,
            "tool_calls": result.tool_calls,
            "api_calls": result.api_calls,
            "input_tokens": result.input_tokens,
            "output_tokens": result.output_tokens,
            "total_tokens": result.total_tokens,
            "cache_read_tokens": result.cache_read_tokens,
            "cache_write_tokens": result.cache_write_tokens,
            "wall_clock_ms": result.wall_clock_ms,
            "estimated_cost_usd": result.estimated_cost_usd,
        }

        # The agent self-reported the whole trophy as trophy.json.
        analysis = self._parse_trophy_json(result)
        agent_findings = [
            {
                "kind": f.get("kind"),
                "title": f.get("title"),
                "description": f.get("description"),
                "anchor": {"call_index": f.get("call_index"), "turn_index": f.get("call_index")},
                "suggestion": f.get("suggestion"),
                "repro": f.get("repro"),
            }
            for f in analysis.get("issues", [])
            if f.get("kind") in ("skill", "language")
        ]
        # Backstop: mine baml errors straight from the turn log; add any the agent
        # didn't already anchor, so friction survives even a thin self-report.
        mined = _mine_baml_errors(turn_log)
        covered = {
            f["anchor"]["call_index"]
            for f in agent_findings
            if f.get("anchor", {}).get("call_index") is not None
        }
        findings = agent_findings + [m for m in mined if m["anchor"]["call_index"] not in covered]

        # Verify each agent-supplied repro by running it through the proxy's baml.
        await self._verify_repros(findings, baml_version, task_id)

        task_completed = analysis.get("task_completed", result.status == "ok")
        outcome = _derive_outcome(result.status, task_completed)

        summary = analysis.get("summary")
        what_failed = analysis.get("what_failed") or []
        if not summary:
            summary = (
                f"Agent wrote no summary; {len(mined)} baml error(s) detected in the run."
                if mined else "Agent wrote no summary."
            )
            if mined and not what_failed:
                what_failed = [m["title"] for m in mined]

        trophy = {
            "taskId": task_id,
            "outcome": outcome,
            "bamlVersion": baml_version,
            "metrics": metrics,
            "hostMetadata": result.host_metadata,
            "transcriptStorageId": transcript_storage_id,
            "turnLog": turn_log,
            "summary": summary,
            "whatWentWell": analysis.get("what_went_well") or [],
            "whatFailed": what_failed,
            "reportMd": analysis.get("report_md") or self._render_report_md(analysis, metrics, outcome),
            "findings": findings,
            "suggestions": analysis.get("suggestions") or [],
            "status": "queued",
        }
        trophy_id = await self.service.create("trophies", trophy)
        await self.service.transition(self.table, task_id, "done")
        log.info("task %s -> trophy (outcome=%s, %d findings, agent status=%s)",
                 task_id, outcome, len(findings), result.status)

        await self._notify(item, outcome, summary, metrics, findings, trophy_id)

    async def _verify_repros(self, findings: list[dict], baml_version: Optional[str],
                             task_id: str) -> None:
        """Verify each finding's repro through the proxy's baml, in place.

        Runs each agent-supplied repro through the proxy's baml and sets `repro`
        (a rendered string) only when it reproduces in the expected way, so
        render_reports_md's "(verified)" label stays honest; otherwise records
        `reproVerified=False` and the actual output for inspection.

        Args:
            findings: Finding dicts to verify; mutated in place with repro
                verification fields.
            baml_version: The baml sha to check against, or None for the current.
            task_id: The owning task id, used to form per-check ids.
        """
        for idx, f in enumerate(findings):
            spec = f.get("repro")
            if not isinstance(spec, dict) or not spec.get("files"):
                f["repro"] = None  # nothing runnable to verify
                continue
            command = str(spec.get("command") or "baml generate")
            files = {str(k): str(v) for k, v in (spec.get("files") or {}).items()}
            try:
                res = await self.proxy.check_baml(CheckBamlRequest(
                    check_id=f"{task_id}-{idx}",
                    files=files,
                    command=command,
                    baml_version=baml_version,
                ))
            except Exception:  # noqa: BLE001
                log.warning("repro check failed for finding %d of %s", idx, task_id)
                f["repro"] = None
                f["reproVerified"] = False
                continue
            should_fail = bool(spec.get("should_fail", True))
            did_fail = res.exit_code != 0
            verified = (not res.timed_out) and (did_fail == should_fail)
            out_tail = ((res.stderr_tail or "") + (res.stdout_tail or "")).strip()[-1200:]
            f["reproVerified"] = verified
            f["reproOutput"] = out_tail
            if verified:
                files_blob = "\n".join(f"--- {k} ---\n{v}" for k, v in files.items())
                f["repro"] = (
                    f"$ {command}\n{files_blob}\n"
                    f"--- output (exit {res.exit_code}) ---\n{out_tail}"
                )
            else:
                f["repro"] = None

    @staticmethod
    def _parse_trophy_json(result) -> dict[str, Any]:
        """Parse the agent's self-reported trophy.json from the run result.

        Prefers the posted trophy.json file; falls back to scraping the last JSON
        object out of the transcript when the file is missing or malformed.

        Args:
            result: The agent run result carrying post_files and transcript.

        Returns:
            The parsed trophy dict, or an empty dict when none can be recovered.
        """
        raw = result.post_files.get("trophy.json")
        if raw:
            try:
                return json.loads(raw)
            except json.JSONDecodeError:
                pass
        scraped = extract_last_json_object(result.transcript or "")
        return scraped if isinstance(scraped, dict) else {}

    @staticmethod
    def _render_report_md(analysis: dict[str, Any], metrics: dict[str, Any], outcome: str) -> str:
        """Render a fallback markdown report when the agent supplied none.

        Args:
            analysis: The parsed trophy dict (summary, what_went_well, what_failed).
            metrics: The run metrics to summarize.
            outcome: The derived trophy outcome.

        Returns:
            A markdown report string.
        """
        lines = [f"# Trophy: {outcome}", "", analysis.get("summary", ""), ""]
        if analysis.get("what_went_well"):
            lines += ["## What went well"] + [f"- {x}" for x in analysis["what_went_well"]] + [""]
        if analysis.get("what_failed"):
            lines += ["## What failed"] + [f"- {x}" for x in analysis["what_failed"]] + [""]
        lines += [
            "## Metrics",
            f"- turns: {metrics.get('turns')}  ·  api_calls: {metrics.get('api_calls')}  ·  "
            f"tool_calls: {metrics.get('tool_calls')}",
            f"- wall_clock_ms: {metrics.get('wall_clock_ms')}  ·  cost: "
            f"${metrics.get('estimated_cost_usd')}",
        ]
        return "\n".join(lines)

    async def _notify(self, item: dict[str, Any], outcome: str, summary: str,
                      metrics: dict[str, Any], findings: list[dict], trophy_id: str) -> None:
        """Post the run result to Slack, linking the trophy on the dashboard.

        No-ops when the task has no Slack channel.

        Args:
            item: The task document (provides Slack channel and thread).
            outcome: The derived trophy outcome.
            summary: The run summary line.
            metrics: The run metrics used to format the stats line.
            findings: The findings list, used for the flagged-issue count.
            trophy_id: Id of the created trophy, used to build the link.
        """
        if not item.get("slackChannel"):
            return
        cost = metrics.get("estimated_cost_usd")
        cost_str = f"${cost:.2f}" if isinstance(cost, (int, float)) else "$-"
        stats = f"{metrics.get('turns', '-')} turns · {metrics.get('api_calls', '-')} api · {cost_str}"
        flagged = f" · {len(findings)} issue(s) flagged" if findings else ""
        link = f"{UI_BASE_URL.rstrip('/')}/runs/{trophy_id}"
        text = (f"*Result: {outcome}.* {summary or ''}\n"
                f"{stats}{flagged}\n"
                f"<{link}|view trophy>")
        await slack_client.post_message(
            SLACK_BOT_TOKEN, item["slackChannel"], text, thread_ts=item.get("slackThreadTs"),
        )


if __name__ == "__main__":
    run_processor(BamlWorker)
