"""RQ jobs for testing BAML fork safety."""

import asyncio
import os
import sys

# Top-level imports - these get loaded in parent process before fork
from baml_client.async_client import b as async_baml_client
from baml_client.sync_client import b as sync_baml_client
from baml_client.tracing import flush


def sync_worker_job():
    """Runs inside RQ worker - uses sync BAML client."""
    print(f"[PID {os.getpid()}] sync_worker_job starting...", flush=True)
    print(f"[PID {os.getpid()}] Calling MakeFoo via sync client...", flush=True)

    try:
        result = sync_baml_client.MakeFoo(
            subject="Subject",
            from_email="from",
            body="body",
        )
        print(f"[PID {os.getpid()}] BAML call succeeded: {result}", flush=True)
    except Exception as e:
        print(f"[PID {os.getpid()}] Error: {e}", flush=True)
        import traceback

        traceback.print_exc()
        return None

    return "Hello2"


async def _async_baml_call():
    """Call the LLM via BAML"""
    print(f"[PID {os.getpid()}] _async_baml_call starting...", flush=True)
    print(f"[PID {os.getpid()}] Calling MakeFoo via async client...", flush=True)

    try:
        result = await async_baml_client.MakeFoo(
            subject="Subject",
            from_email="from",
            body="body",
        )
        print(f"[PID {os.getpid()}] BAML call succeeded: {result}", flush=True)
    except Exception as e:
        print(f"[PID {os.getpid()}] Error: {e}", flush=True)
        import traceback

        traceback.print_exc()
        return None

    print(f"[PID {os.getpid()}] Flushing traces...", flush=True)
    flush()

    return "Hello2"


def async_worker_job():
    """Runs inside RQ worker - wraps async call with asyncio.run()."""
    print(f"[PID {os.getpid()}] async_worker_job starting...", flush=True)
    sys.stdout.flush()
    sys.stderr.flush()

    result = asyncio.run(_async_baml_call())
    print(
        f"[PID {os.getpid()}] async_worker_job completed with result: {result}",
        flush=True,
    )
    return result
