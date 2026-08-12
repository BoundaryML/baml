"""End-to-end pipeline tests (the real guard).

These boot the stack via the `bench_stack` fixture (Convex container + api/ingress/
fake_proxy host processes) and run the existing drivers, which exercise the actual
claim-loop logic in-process:
  * drive_pipeline - task -> BamlWorker -> trophy -> BamlDedup -> issue -> NotionPush
  * drive_ingress  - /bug, signed /slack/events, /notion/webhook approve, FixDispatch

The drivers are imported lazily (inside each test) because `drive_ingress` reads
INGRESS_URL / SLACK_SIGNING_SECRET at import time - by then `bench_stack` has set them.
"""

import importlib

import pytest

pytestmark = pytest.mark.integration


async def test_pipeline_e2e(bench_stack):
    """Run the task-to-issue pipeline driver against the booted stack.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    mod = importlib.import_module("tests.drive_pipeline")
    await mod.main()  # asserts internally; prints PIPELINE_OK


async def test_ingress_e2e(bench_stack):
    """Run the ingress + fix-dispatch driver against the booted stack.

    Args:
        bench_stack: Session fixture providing the running api/ingress/proxy URLs.
    """
    mod = importlib.import_module("tests.drive_ingress")
    await mod.main()  # asserts internally; prints INGRESS_OK
