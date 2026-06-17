"""Bug-verify processor: re-checks reported bugs against the latest nightly.

A singleton poller (no queue claims, so it never disturbs the human review
lifecycle on the board). Each cycle it finds open/confirmed issues that have
not been verified against the newest ready nightly, runs a verification
agent per issue with that baml pinned on PATH, and:

- still broken: stamps verifiedAt / verifyBamlVersion (and brokeIn, derived
  from the first evidence run) so the dashboard shows "last verified
  against X".
- fixed (high confidence only): additionally stamps fixedIn, transitions
  the issue to ``closed``, marks it linear-dirty for the regular re-sync,
  flips its Linear card status, and leaves a comment with the evidence.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import socket
import time
import uuid
from typing import Any, Optional

from bench_core.jsonl import extract_last_json_object
from bench_core import linear_client as lc
from bench_core.linear_client import LinearClient
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import RunAgentRequest
from bench_core.service_client import ServiceClient

from .prompts import VERIFY_SYSTEM_PROMPT, VERIFY_USER_PROMPT

log = logging.getLogger("bug_verify")

BUG_VERIFY_MODEL = os.environ.get("BUG_VERIFY_MODEL", "claude-sonnet-4-6")
BUG_VERIFY_MAX_TURNS = int(os.environ.get("BUG_VERIFY_MAX_TURNS", "16"))
BUG_VERIFY_TIMEOUT_SECS = int(os.environ.get("BUG_VERIFY_TIMEOUT_SECS", "900"))
# How many issues to verify per cycle (cost control: each is an agent run).
BUG_VERIFY_BATCH = int(os.environ.get("BUG_VERIFY_BATCH", "6"))
BUG_VERIFY_POLL_SECS = int(os.environ.get("BUG_VERIFY_POLL_SECS", "900"))
BAML_CHANNEL = os.environ.get("BAML_CHANNEL", "nightly")
LINEAR_API_KEY = os.environ.get("ATB_LINEAR_TOKEN", "")
# Board label for fixed issues; the merged label matches LinearPush's closed mapping.
LINEAR_STATUS_FIXED = lc.LINEAR_STATUS_MERGED

# Issue lifecycle states worth re-checking (on the board, not yet closed).
VERIFIABLE_STATUSES = ("open", "confirmed")


def ref_label(ref: Optional[str]) -> Optional[str]:
    """Strip the build-ref prefix for the readable version label.

    Args:
        ref: A bamlBuilds ref like ``baml-language-0.12.1-nightly.20260611.b``.

    Returns:
        The readable version (``0.12.1-nightly.20260611.b``), or None.
    """
    return ref.removeprefix("baml-language-") if ref else None


def issue_json(issue: dict[str, Any]) -> str:
    """Render the agent-relevant issue fields as the issue.json input file.

    Args:
        issue: The issue row being verified.

    Returns:
        Pretty-printed JSON with title/kind/category/description/suggestion/repro.
    """
    return json.dumps({
        "title": issue.get("title"),
        "kind": issue.get("kind"),
        "category": issue.get("category"),
        "description": issue.get("description"),
        "suggestion": issue.get("suggestion"),
        "repro": issue.get("repro"),
    }, indent=2)


class BugVerify:
    """Poll-loop verifier: one agent run per (issue, nightly) pair."""

    role = "bug-verify"

    def __init__(self, service: ServiceClient):
        """Bind clients and mint the worker id for the dashboard roster.

        Args:
            service: ServiceClient for all Convex reads/writes.
        """
        self.service = service
        self.proxy = ProxyClient.from_env()
        self.linear = LinearClient(LINEAR_API_KEY) if LINEAR_API_KEY else None
        self.id = f"{self.role}-{socket.gethostname()}-{os.getpid()}-{uuid.uuid4().hex[:6]}"

    # ---- presence (observability only, never load-bearing) ----
    async def _presence(self, status: str, item_id: Optional[str] = None) -> None:
        """Mirror this worker into the dashboard roster, best-effort.

        Args:
            status: "idle" or "busy".
            item_id: The issue being verified when busy.
        """
        try:
            await self.service.worker_heartbeat(self.id, self.role, status, item_id)
        except Exception:  # noqa: BLE001
            log.debug("presence write failed (ignored)", exc_info=True)

    # ---- one polling cycle ----
    async def cycle(self) -> None:
        """Verify up to BUG_VERIFY_BATCH issues against the newest ready nightly."""
        builds = await self.service.baml_list_ready(channel=BAML_CHANNEL, limit=1)
        if not builds:
            log.info("bug-verify: no ready %s build; skipping cycle", BAML_CHANNEL)
            return
        build = builds[0]
        sha, label = build.get("sha"), ref_label(build.get("ref"))
        if not sha or not label:
            return

        candidates: list[dict[str, Any]] = []
        for status in VERIFIABLE_STATUSES:
            rows = await self.service.list(
                "issues", field="status", value=status,
                index="by_status_created", limit=300,
            )
            candidates.extend(
                r for r in rows if r.get("verifyBamlVersion") != label
            )
        if not candidates:
            log.info("bug-verify: everything already verified against %s", label)
            return
        # Oldest verification first so every issue cycles through eventually.
        candidates.sort(key=lambda r: r.get("verifiedAt") or 0)

        log.info(
            "bug-verify: %d issue(s) unverified against %s; running %d",
            len(candidates), label, min(BUG_VERIFY_BATCH, len(candidates)),
        )
        for issue in candidates[:BUG_VERIFY_BATCH]:
            await self._presence("busy", issue["_id"])
            try:
                await self.verify_issue(issue, sha, label)
            except Exception:  # noqa: BLE001
                log.exception("bug-verify: verification failed for %s", issue["_id"])
            finally:
                await self._presence("idle", None)

    async def verify_issue(self, issue: dict[str, Any], sha: str, label: str) -> None:
        """Run the verification agent for one issue and record the verdict.

        Args:
            issue: The issue row to verify.
            sha: Build sha the proxy pins on PATH.
            label: Readable version label for that sha.
        """
        issue_id = issue["_id"]
        files = {"issue.json": issue_json(issue)}
        repro = (issue.get("repro") or "").strip()
        if repro:
            files["repro.baml"] = repro

        req = RunAgentRequest(
            cell_id=f"bug-verify-{issue_id}-{int(time.time())}",
            model=BUG_VERIFY_MODEL,
            max_turns=BUG_VERIFY_MAX_TURNS,
            prompt=VERIFY_USER_PROMPT,
            system_prompt=VERIFY_SYSTEM_PROMPT,
            files=files,
            baml_version=sha,
            # install the official BAML skill matching this exact baml, the
            # same way normal task runs get it (baml agent install on the proxy)
            install_skill=True,
            post_file_patterns=["verdict.json"],
            max_file_bytes=256 * 1024,
            invocation_timeout_secs=BUG_VERIFY_TIMEOUT_SECS,
        )
        result = await self.proxy.run_agent(req, timeout=BUG_VERIFY_TIMEOUT_SECS + 120)
        verdict = self._parse_verdict(result)

        now_ms = int(time.time() * 1000)
        patch: dict[str, Any] = {"verifiedAt": now_ms, "verifyBamlVersion": label}
        if not issue.get("brokeIn"):
            broke_in = await self._broke_in(issue)
            if broke_in:
                patch["brokeIn"] = broke_in

        fixed = (
            verdict.get("still_broken") is False
            and verdict.get("confidence") == "high"
        )
        if not fixed:
            await self.service.update("issues", issue_id, patch)
            log.info(
                "bug-verify: %s still broken on %s (confidence=%s)",
                issue_id, label, verdict.get("confidence"),
            )
            return

        evidence = (verdict.get("evidence") or "").strip()
        patch.update({"fixedIn": label, "linearSyncStatus": "dirty"})
        await self.service.update("issues", issue_id, patch)
        # release_claim=False: the issue is not claimed by us, and another
        # worker may hold a claim on a different queue field.
        await self.service.transition(
            "issues", issue_id, "closed", field="status", release_claim=False
        )
        log.info("bug-verify: %s FIXED in %s", issue_id, label)
        await self._linear_fixed(issue, label, evidence)

    async def _linear_fixed(self, issue: dict[str, Any], label: str, evidence: str) -> None:
        """Flip the issue's Linear card to the merged status and leave evidence.

        Best-effort: the dirty flag already queues a full LinearPush re-sync (which
        also maps closed -> merged), so a Linear failure here never blocks the
        verdict — this just surfaces the merge on the board immediately.

        Args:
            issue: The fixed issue (its linearIssueId is used when present).
            label: Version the fix was verified against.
            evidence: The agent's one-paragraph observation.
        """
        linear_id = issue.get("linearIssueId")
        if not self.linear or not linear_id:
            return
        try:
            await self.linear.set_status(linear_id, LINEAR_STATUS_FIXED)
            comment = f"Verified fixed in baml {label}."
            if evidence:
                comment += f" {evidence}"
            await self.linear.add_comment(linear_id, comment)
        except Exception:  # noqa: BLE001
            log.exception("bug-verify: linear update failed for issue %s", linear_id)

    async def _broke_in(self, issue: dict[str, Any]) -> Optional[str]:
        """Resolve the version the bug was first observed on, via its evidence.

        Args:
            issue: The issue whose first evidence run to inspect.

        Returns:
            The readable version label of the first evidence run's baml, or None.
        """
        evidence = issue.get("evidence") or []
        trophy_id = next((e.get("trophyId") for e in evidence if e.get("trophyId")), None)
        if not trophy_id:
            return None
        try:
            trophy = await self.service.get("trophies", trophy_id)
            sha = (trophy or {}).get("bamlVersion")
            if not sha or sha == "coldstart":
                return None
            builds = await self.service.list(
                "bamlBuilds", field="sha", value=sha, index="by_sha", limit=1,
            )
            return ref_label(builds[0].get("ref")) if builds else None
        except Exception:  # noqa: BLE001
            log.debug("bug-verify: brokeIn resolution failed", exc_info=True)
            return None

    @staticmethod
    def _parse_verdict(result) -> dict[str, Any]:
        """Extract the verdict object the agent produced.

        Prefers the posted verdict.json and falls back to the last JSON object
        in the transcript; anything unparseable counts as still broken.

        Args:
            result: The agent run result carrying post_files and transcript.

        Returns:
            The verdict dict (possibly empty, which reads as still broken).
        """
        raw = result.post_files.get("verdict.json")
        if raw:
            try:
                data = json.loads(raw)
                if isinstance(data, dict):
                    return data
            except json.JSONDecodeError:
                pass
        scraped = extract_last_json_object(result.transcript or "")
        return scraped if isinstance(scraped, dict) else {}

    async def run(self) -> None:
        """Run cycles forever on the poll interval, with presence beats."""
        log.info("%s starting: poll=%ss batch=%s", self.id, BUG_VERIFY_POLL_SECS, BUG_VERIFY_BATCH)
        await self._presence("idle")
        while True:
            try:
                await self.cycle()
            except Exception:  # noqa: BLE001
                log.exception("bug-verify: cycle failed")
            await self._presence("idle")
            await asyncio.sleep(BUG_VERIFY_POLL_SECS)


def main() -> None:
    """Entry point: build the client from env and run the verifier loop."""
    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"))
    service = ServiceClient(
        base_url=os.environ["SERVICE_URL"],
        token=os.environ.get("ATB_SERVICE_TOKEN", ""),
    )

    async def _main() -> None:
        verifier = BugVerify(service)
        try:
            await verifier.run()
        finally:
            await service.aclose()

    asyncio.run(_main())


if __name__ == "__main__":
    main()
