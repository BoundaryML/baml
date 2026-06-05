"""System test: the entire pipeline, end to end.

Starting from the real public entry point (``POST /bug`` to ingress), drives the
whole chain — worker -> dedup -> notion-push -> approval via ``/notion/webhook`` ->
FixDispatch — and asserts the bug ends up dispatched for a fix. Runs against the
deterministic ``tests/fake_proxy.py`` stub (no real Claude, no secrets). Marked
``system``; self-skips without Docker (see ``bench_stack``).
"""

from __future__ import annotations

import os

import httpx
import pytest

from bench_core.service_client import ServiceClient
from tests import pipeline_steps as steps

pytestmark = pytest.mark.system


async def test_full_pipeline_bug_to_fix(bench_stack):
    """Drive bug -> task -> trophy -> issue -> notion -> approve -> fix end to end.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    service = ServiceClient(bench_stack["api"], os.environ.get("SERVICE_TOKEN", ""))
    try:
        async with httpx.AsyncClient(timeout=30.0) as http:
            # 1. real entry point: a bug report creates a queued task
            task_id = await steps.post_bug_via_ingress(http, bench_stack["ingress"], service)
            # 2. worker runs the (stub) agent and assembles a trophy
            await steps.run_worker_assert_trophy(service, task_id)
            # 3. dedup classifies the finding into a language issue
            issue = await steps.run_dedup_assert_issue(service)
            # 4. notion-push (no-creds path) syncs + confirms the issue
            await steps.run_notion_push_assert_synced(service, issue["_id"])
            # 5. approval arrives back through the notion webhook
            await steps.approve_issue_via_webhook(
                http, bench_stack["ingress"], service, issue["_id"], page_id="pg-system-1")
            # 6. fix dispatch launches a Cursor cloud agent
            await steps.run_fixdispatch_assert_launch(service, issue["_id"])
    finally:
        await service.aclose()
