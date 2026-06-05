"""BAML worker processor: claim a task, run the agent (with the latest nightly
baml on PATH and the BAML skill injected). The agent assembles the whole trophy
itself as one verbose `trophy.json` (report + issues + suggestions); the worker
verifies its repros, creates the trophy, and finishes the task. One agent per DB,
no reviewer.

Each run polls for the latest nightly release (`baml_update`) and blocks until
that exact build is uploaded to the builder before running, so every run uses the
freshest nightly rather than whatever the daily cron last pinned.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import time
from pathlib import Path
from typing import Any, Optional

from bench_core import slack_client
from bench_core.channels import DEFAULT_CHANNEL
from bench_core.jsonl import extract_last_json_object
from bench_core.prices import prices_for
from bench_core.processor import Processor, run_processor
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import CheckBamlRequest, RunAgentRequest

from .prompts import COLD_START_SYSTEM_PROMPT, WORKER_SYSTEM_PROMPT

log = logging.getLogger("baml_worker")

CLAUDE_MODEL = os.environ.get("CLAUDE_MODEL", "claude-sonnet-4-6")
CLAUDE_MAX_TURNS = int(os.environ.get("CLAUDE_MAX_TURNS", "40"))
AGENT_TIMEOUT_SECS = int(os.environ.get("WORKER_AGENT_TIMEOUT_SECS", "3600"))
# How long a run waits for its pinned nightly to be built+uploaded before
# falling back to whatever ready build is current, and how often it polls.
BAML_BUILD_WAIT_SECS = int(os.environ.get("BAML_BUILD_WAIT_SECS", "300"))
BAML_BUILD_POLL_SECS = float(os.environ.get("BAML_BUILD_POLL_SECS", "5"))
# How many ready builds a `[baml=N]` pin can select from (newest-first); mirrors
# the api's retention so the index range matches what's kept in the bucket.
BAML_KEEP_RELEASES = int(os.environ.get("BAML_KEEP_RELEASES", "5"))
SLACK_BOT_TOKEN = os.environ.get("ATB_SLACK_BOT_TOKEN", "")
UI_BASE_URL = os.environ.get("UI_BASE_URL", "https://bench3-ui.fly.dev")
BAML_SKILL_PATH = os.environ.get("BAML_SKILL_PATH", "")   # single combined file
BAML_SKILL_DIR = os.environ.get("BAML_SKILL_DIR", "")     # dir of */SKILL.md (preferred)
POST_FILE_PATTERNS = ["**/*.baml", "baml_src/**/*.baml", "*.md", "trophy.json"]
# Injected/scratch files captured by POST_FILE_PATTERNS that are not project
# artifacts the agent authored, so they're excluded from "Files created".
_NON_PROJECT_FILES = {"SKILL.md", "trophy.json", "reports.md", "open_issues.json", "issues.json"}

_SKILL_CACHE: Optional[str] = None
_SKILL_LOADED = False


def _project_files(post_files: dict[str, str]) -> dict[str, str]:
    """Filter captured post-run files down to the agent's project artifacts.

    Keeps the .baml/project files the agent created and drops injected or scratch
    files (the SKILL.md we injected, the self-reported trophy.json, dedup
    scratch), so the trophy's "Files created" view shows only real artifacts.

    Args:
        post_files: The raw {path: content} map captured by the proxy.

    Returns:
        A {path: content} map of project artifacts only.
    """
    out: dict[str, str] = {}
    for path, content in (post_files or {}).items():
        if path.rsplit("/", 1)[-1] in _NON_PROJECT_FILES:
            continue
        out[path] = content
    return out


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

    async def _current_for_channel(self, channel: str) -> Optional[str]:
        """Return the newest ready sha on a channel (last-resort fallback).

        Args:
            channel: The release channel to pick the newest ready build from.

        Returns:
            The newest ready build's sha on the channel, else the newest ready
            build of any channel, else None.
        """
        builds = await self.service.baml_list_ready(channel=channel, limit=1)
        if builds:
            return builds[0]["sha"]
        baml = await self.service.baml_current()
        return baml["sha"] if baml else None

    async def _resolve_baml_version(self, item: dict[str, Any]) -> Optional[str]:
        """Resolve which baml sha a run should use.

        Uses the task's channel (`bamlChannel`, default nightly). When the task
        pins a build (`bamlPin`, a 1-based index into that channel's newest ready
        builds), resolve that exact build. Otherwise refresh the channel's latest
        release and block until its build is ready. On a bad/out-of-range pin,
        timeout, or failure, fall back to the channel's current build so a run is
        never hard-failed.

        Args:
            item: The claimed task document (may carry `bamlChannel`/`bamlPin`).

        Returns:
            The sha the run should use, or None when no build is available.
        """
        channel = item.get("bamlChannel") or DEFAULT_CHANNEL
        pin = (item.get("bamlPin") or "").strip()
        if pin:
            builds = await self.service.baml_list_ready(channel=channel, limit=BAML_KEEP_RELEASES)
            try:
                idx = int(pin)
            except ValueError:
                idx = 0
            if 1 <= idx <= len(builds):
                row = builds[idx - 1]
                log.info("task pinned baml %s #%d -> %s (%s)", channel, idx, row["sha"], row.get("ref"))
                return row["sha"]
            log.warning(
                "task baml pin %r out of range (1..%d ready on %s); using latest",
                pin, len(builds), channel,
            )

        try:
            resp = await self.service.baml_update(channel)
        except Exception:  # noqa: BLE001
            log.exception("baml update (%s) failed; falling back to current build", channel)
            return await self._current_for_channel(channel)

        target_sha = resp.get("sha") or resp.get("enqueued")
        if resp.get("built"):
            return target_sha  # channel's latest already built+uploaded

        deadline = time.monotonic() + BAML_BUILD_WAIT_SECS
        log.info("waiting for %s %s to build (version=%s)", channel, target_sha, resp.get("version"))
        while target_sha and time.monotonic() < deadline:
            status = await self.service.baml_build_status(target_sha)
            if status == "ready":
                return target_sha
            if status == "failed":
                log.warning("%s %s build failed; falling back to current", channel, target_sha)
                break
            await asyncio.sleep(BAML_BUILD_POLL_SECS)
        else:
            if target_sha:
                log.warning(
                    "%s %s not ready after %ds; falling back to current",
                    channel, target_sha, BAML_BUILD_WAIT_SECS,
                )

        return await self._current_for_channel(channel)

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
        cold = bool(item.get("coldStart"))
        pin = (item.get("bamlPin") or "").strip()
        channel = item.get("bamlChannel") or DEFAULT_CHANNEL
        # Mode note appended to the pickup ack so the requester sees how their
        # [coldstart]/[baml=N]/[canary] directives were interpreted.
        mode_note = ""
        if cold:
            mode_note = "  _(cold start — installing baml from quickstart)_"
        elif pin:
            mode_note = f"  _(baml {channel} #{pin})_"
        elif channel != DEFAULT_CHANNEL:
            mode_note = f"  _({channel} latest)_"

        # Slack "on it" ack at pickup time, linking to the task on the dashboard.
        if item.get("slackChannel"):
            link = f"{UI_BASE_URL.rstrip('/')}/tasks/{task_id}"
            await slack_client.post_message(
                SLACK_BOT_TOKEN,
                item["slackChannel"],
                f"_Running `{item['prompt'][:120]}` …_  <{link}|track on dashboard>{mode_note}",
                thread_ts=item.get("slackThreadTs"),
            )

        # Mode branch: cold start withholds baml + the skill (the agent installs
        # baml itself and onboards from the quickstart); warm/pinned runs resolve
        # a sha and inject the skill as usual.
        files: dict[str, str] = {}
        if cold:
            baml_version = None
            system_prompt = COLD_START_SYSTEM_PROMPT
            await self.service.update(self.table, task_id, {"bamlVersion": "coldstart"})
        else:
            # Poll for the pinned/latest nightly and block until it's built.
            baml_version = await self._resolve_baml_version(item)
            system_prompt = WORKER_SYSTEM_PROMPT
            skill = _load_skill()
            if skill:
                files["SKILL.md"] = skill
            # Record the baml version so the in-flight task page shows which baml
            # this run is using.
            if baml_version:
                await self.service.update(self.table, task_id, {"bamlVersion": baml_version})
        prices = prices_for(CLAUDE_MODEL)

        req = RunAgentRequest(
            cell_id=task_id,
            model=CLAUDE_MODEL,
            max_turns=CLAUDE_MAX_TURNS,
            prompt=item["prompt"],
            system_prompt=system_prompt,
            files=files,
            prices=prices,
            baml_version=baml_version,
            post_file_patterns=POST_FILE_PATTERNS,
            invocation_timeout_secs=AGENT_TIMEOUT_SECS,
        )
        log.info("running task %s (baml=%s, channel=%s, cold=%s, pin=%s)",
                 task_id, baml_version, channel, cold, pin or None)
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
        # The project files the agent created (.baml + other artifacts), shown on
        # the run page and counted into the metric grid.
        files_created = _project_files(result.post_files)
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
            "files_touched": len(files_created),
            "loc_changed": sum(len((c or "").splitlines()) for c in files_created.values()),
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
            "filesCreated": files_created,
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
