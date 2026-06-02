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


async def test_fixdispatch_launches_cursor_on_approved_issue(bench_stack):
    """Approving an issue via /notion/webhook then draining FixDispatch launches Cursor.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = _service(bench_stack)
    try:
        async with httpx.AsyncClient(timeout=20.0) as http:
            iid = await steps.seed_synced_issue(service, kind="skill", page_id="pg-fix-1")
            await steps.approve_issue_via_webhook(
                http, bench_stack["ingress"], service, iid, page_id="pg-fix-1")
            await steps.run_fixdispatch_assert_launch(service, iid)
    finally:
        await service.aclose()
