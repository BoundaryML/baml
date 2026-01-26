#!/usr/bin/env python
"""
Minimal BAML observability test.

Test 1: Sync call on main thread (works, shows in Boundary Studio)
Test 2: Async call in RQ worker (works, but MISSING from Boundary Studio)

To run:
    1. Start Redis (if not running):
       redis-server

    2. Start RQ worker in one terminal:
       PYTHONPATH=. uv run python app/manage.py rqworker async

    3. Run tests in another terminal:
       PYTHONPATH=. uv run python app/manage.py shell
       >>> from app.manage import test_sync_main, test_async_worker
       >>> test_sync_main()
       >>> test_async_worker()

    4. Check Boundary Studio for:
       - [TEST 1 - SYNC MAIN] ← runs fine, show up in Boundary Studio
       - [TEST 2 - ASYNC WORKER] ← runs fine, but missing from Boundary Studio
"""

import os
import sys
import time

# Setup Django before importing django_rq
os.environ.setdefault("DJANGO_SETTINGS_MODULE", "app.settings")

import django

django.setup()

import django_rq

from core.constants import QueueNames
from core.utils.general_utils import sleep
from baml_client.async_client import b as async_baml_client
from baml_client.tracing import flush


def test_sync_main():
    """BAML call on main thread - this works and shows in Boundary Studio."""
    from baml_client.sync_client import b as baml_client

    print("\nTesttt 1: Sync on main thread...xxx")
    try:
        result = baml_client.MakeFoo(
            subject="Something",
            from_email="test@greghale.io",
            body="Testing",
        )
        print("Hello")
    except Exception as e:
        print(f"Error: {e}")
        return None
    print(f"Result: {result}")


async def _async_baml_call():
    """Call the LLM via BAML"""

    from baml_client.tracing import flush

    try:
        result = await async_baml_client.MakeFoo(
            subject="Subject",
            from_email="from",
            body="body",
        )
        print("Hello")
    except Exception as e:
        print(f"Error: {e}")
        return None

    # Force the tracing system to finish publishing.
    flush()
    sleep(4)

    return "Hello2"


def _async_worker_job():
    """Runs inside RQ worker - wraps async call with asyncio.run()."""
    import asyncio

    return asyncio.run(_async_baml_call())


def test_async_worker():
    """BAML async call in RQ worker - works but missing from Boundary Studio!"""
    from core.jobs import async_worker_job

    print("\nTest 2: Async in RQ worker...")
    queue = django_rq.get_queue(QueueNames.ASYNC.value)
    job = queue.enqueue(async_worker_job)
    print(f"Job ID: {job.id}")

    # Wait for completion
    start = time.time()
    while time.time() - start < 30:
        job.refresh()
        if job.is_finished:
            print(f"Result: {job.result}")
            return
        elif job.is_failed:
            print(f"Failed: {job.exc_info}")
            return
        sleep(0.5)

    print("Timed out")


if __name__ == "__main__":
    from django.core.management import execute_from_command_line

    # Check for custom test command
    if len(sys.argv) > 1 and sys.argv[1] == "test":
        test_async_worker()
    else:
        execute_from_command_line(sys.argv)
