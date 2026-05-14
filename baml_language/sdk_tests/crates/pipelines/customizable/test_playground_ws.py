"""End-to-end check that Python-triggered BAML calls broadcast the
`callFunction` / `callFunctionResult` bracket on the playground WS.

Importing ``baml_sdk`` triggers ``bridge_cffi::engine::initialize_runtime``,
which prints ``BAML playground: http://localhost:{port}`` to stderr. We
capture that with ``capfd`` to discover the playground port, then attach
a WebSocket client and trigger a real Python-side function call. The
fix in ``bridge_python::runtime::BamlRuntime::{call_function,
call_function_sync}`` is what makes the bracket arrive on this channel.

Network-dependent: ``HandleOrder`` makes three ``httpbin.org/delay/3``
calls. Set ``BAML_SKIP_PLAYGROUND_WS=1`` to skip in offline envs.
"""

from __future__ import annotations

import asyncio
import json
import os
import re
import threading
import time

import pytest
import websockets


@pytest.mark.skipif(
    "BAML_SKIP_PLAYGROUND_WS" in os.environ,
    reason="set BAML_SKIP_PLAYGROUND_WS=1 to skip (e.g. CI without network)",
)
def test_python_triggered_call_broadcasts_call_function_and_result(capfd):
    import baml_sdk  # noqa: F401 — side-effect import wires the runtime

    captured = capfd.readouterr().err
    m = re.search(r"BAML playground: http://localhost:(\d+)", captured)
    assert m, f"could not find playground port in stderr: {captured!r}"
    port = int(m.group(1))

    async def run() -> None:
        async with websockets.connect(f"ws://localhost:{port}/api/ws") as ws:
            # Drain Ready + initial snapshot messages. Stop when the server
            # pauses; ~100ms is plenty on localhost.
            try:
                while True:
                    await asyncio.wait_for(ws.recv(), timeout=0.1)
            except asyncio.TimeoutError:
                pass

            # `HandleOrder` is blocking sync — run it in a worker thread so
            # the WS reader stays live.
            result_holder: dict = {}

            def _call() -> None:
                try:
                    result_holder["value"] = baml_sdk.HandleOrder(id=1)
                except Exception as e:  # noqa: BLE001 — surface to assertion below
                    result_holder["error"] = repr(e)

            t = threading.Thread(target=_call, daemon=True)
            t.start()

            seen_call: int | None = None
            seen_result: int | None = None
            deadline = time.monotonic() + 30.0
            while (seen_call is None or seen_result is None) and time.monotonic() < deadline:
                msg = json.loads(await asyncio.wait_for(ws.recv(), timeout=15.0))
                ty = msg.get("type")
                if ty == "callFunction" and msg.get("name") == "user.HandleOrder":
                    seen_call = msg["id"]
                elif ty == "callFunctionResult" and seen_call is not None and msg["id"] == seen_call:
                    seen_result = msg["id"]

            t.join(timeout=2.0)

            assert seen_call is not None, (
                "callFunction not delivered for Python-triggered HandleOrder"
            )
            assert seen_result == seen_call, (
                f"callFunctionResult missing or id mismatch "
                f"(call={seen_call}, result={seen_result})"
            )
            assert "error" not in result_holder, result_holder.get("error")

    asyncio.run(run())
