"""Per-hop integration tests: each boots the shared stack (the ``bench_stack``
fixture: Convex container + api/ingress/fake_proxy host processes) and exercises a
single pipeline hop against the live api/Convex plus the deterministic fake proxy.

These verify the claim-loop wiring of each stage in isolation; the full chain is
covered by ``test_system_pipeline.py``. All are marked ``integration`` and self-skip
when Docker is unavailable (see ``bench_stack``).
"""

from __future__ import annotations

import os

import httpx
import pytest

from bench_core.service_client import ServiceClient
from tests import pipeline_steps as steps

pytestmark = pytest.mark.integration


def _service(bench_stack) -> ServiceClient:
    """Build a ServiceClient pointed at the stack's api with its bearer token.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.

    Returns:
        A ServiceClient for the running api.
    """
    return ServiceClient(bench_stack["api"], os.environ.get("SERVICE_TOKEN", ""))


async def test_worker_creates_trophy(bench_stack):
    """BamlWorker claims a queued task, runs the stub agent, and assembles a trophy.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    try:
        task_id = await steps.create_bug_task(service)
        await steps.run_worker_assert_trophy(service, task_id)
        # Warm-run contract: the worker asks the proxy to `baml agent install`
        # the official skills instead of injecting a SKILL.md itself.
        async with httpx.AsyncClient(timeout=20.0) as http:
            r = await http.get(f"{bench_stack['proxy']}/run-agent-requests")
        req = r.json()[task_id]
        assert req["install_skill"] is True
        assert "SKILL.md" not in (req.get("files") or {})
    finally:
        await service.aclose()


async def test_dedup_creates_issue(bench_stack):
    """BamlDedup batches a queued trophy and upserts the deduplicated language issue.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    try:
        task_id = await steps.create_bug_task(service)
        await steps.run_worker_assert_trophy(service, task_id)
        await steps.run_dedup_assert_issue(service)
    finally:
        await service.aclose()


async def test_ingress_bug_creates_task(bench_stack):
    """POST /bug to ingress creates a queued bug_report task.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    try:
        async with httpx.AsyncClient(timeout=20.0) as http:
            await steps.post_bug_via_ingress(http, bench_stack["ingress"], service)
    finally:
        await service.aclose()


async def test_ingress_signed_slack_creates_task(bench_stack):
    """A signed /slack/events app_mention creates a slack task; a bad signature is 401.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    secret = os.environ["SLACK_SIGNING_SECRET"]
    try:
        async with httpx.AsyncClient(timeout=20.0) as http:
            await steps.post_signed_slack_assert_task(
                http, bench_stack["ingress"], secret, service)
    finally:
        await service.aclose()


async def test_arena_cohort_fanin_and_compare(bench_stack):
    """A skill-arena cohort fans in once its members finish, then compares them.

    Members run to HELD (cohort_member) trophies; the cohort stays pending until the
    reconciler flips it to queued; CohortCompare then emits a queued cohort trophy
    and releases the member trophies.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    try:
        cohort_id, member_ids = await steps.create_cohort_with_members(service)
        await steps.run_cohort_members(service, member_ids)
        # The cohort must NOT advance until the reconciler runs.
        c = await service.get("cohorts", cohort_id)
        assert c["status"] == "pending", c["status"]
        await steps.reconcile_cohort_assert_queued(service, cohort_id)
        await steps.run_cohort_compare_assert_trophy(service, cohort_id, member_ids)
    finally:
        await service.aclose()


async def test_arena_cohort_advances_when_last_member_failed(bench_stack):
    """The reconciler advances a cohort even if a member is failed with no worker run.

    This is the reap-deadlock guard: a member reaped to ``failed`` (simulated here by
    forcing the status, since no worker runs it) still counts as terminal, so the
    cohort is never stranded — proving the sweep beats a worker-driven barrier.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    try:
        cohort_id, member_ids = await steps.create_cohort_with_members(
            service, branches=["main", "exp-a"])
        # Simulate the last member reaped to failed (no worker will process it).
        await service.update("tasks", member_ids[-1], {"status": "failed"})
        # Drain the worker for the remaining queued member(s).
        from services.baml_worker.__main__ import BamlWorker
        await BamlWorker(service)._drain()
        # The reconciler must still advance the cohort (failed is terminal).
        await steps.reconcile_cohort_assert_queued(service, cohort_id)
    finally:
        await service.aclose()


async def test_fixdispatch_launches_cursor_on_approved_issue(bench_stack):
    """Approving an issue via /linear/webhook then draining FixDispatch launches Cursor.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    try:
        async with httpx.AsyncClient(timeout=20.0) as http:
            iid = await steps.seed_synced_issue(service, kind="skill", linear_id="li-fix-1")
            await steps.approve_issue_via_webhook(
                http, bench_stack["ingress"], service, iid, linear_id="li-fix-1")
            await steps.run_fixdispatch_assert_launch(service, iid)
    finally:
        await service.aclose()


async def test_cursor_tracker_advances_tocursor_to_prprep(bench_stack):
    """After dispatch, the tracker finds the agent's PR and moves the issue to prprep.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    try:
        async with httpx.AsyncClient(timeout=20.0) as http:
            iid = await steps.seed_synced_issue(service, kind="language", linear_id="li-track-1")
            await steps.approve_issue_via_webhook(
                http, bench_stack["ingress"], service, iid, linear_id="li-track-1")
            await steps.run_fixdispatch_assert_launch(service, iid)
            await steps.run_tracker_assert_prprep(service, iid)
    finally:
        await service.aclose()
