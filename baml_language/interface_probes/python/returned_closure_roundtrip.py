"""Probe ownership and pass-back behavior of an engine-owned live closure.

Run from baml_language/sdks/python after building the editable bridge:

    uv run python ../../interface_probes/python/returned_closure_roundtrip.py

The final Apply call intentionally records a current bridge failure. The
returned BamlClosure is treated as a new Python host callable when passed back
to BAML, so its __call__ performs a nested synchronous runtime call from a
Tokio worker.
"""

from __future__ import annotations

import gc

from baml_bridge import BamlRuntime, call_function_sync, flush_events
from baml_bridge.baml_py import _live_handle_count


SOURCE = """
function MakeCounter(start: int) -> () -> int throws never {
    let current = start;
    return () -> int {
        current += 1;
        current
    }
}

function Apply(callback: () -> int) -> int {
    callback()
}
"""


def main() -> None:
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    baseline = _live_handle_count()
    counter = call_function_sync(runtime, "MakeCounter", {"start": 40}).result()
    after_direct_return = _live_handle_count()

    direct = [counter(), counter()]
    assert direct == [41, 42]
    assert after_direct_return == baseline + 1

    del counter
    flush_events()
    gc.collect()
    after_direct_drop = _live_handle_count()
    assert after_direct_drop == baseline

    pass_back_counter = call_function_sync(
        runtime, "MakeCounter", {"start": 100}
    ).result()

    error_text: str | None = None
    try:
        call_function_sync(runtime, "Apply", {"callback": pass_back_counter}).result()
    except BaseException as error:
        error_text = f"{type(error).__name__}: {error}"
        # Keep no exception/traceback/frame alive while measuring handles.
        error.__traceback__ = None

    assert error_text is not None
    assert (
        "SdkPanic" in error_text
        or "Cannot start a runtime from within a runtime" in error_text
    )

    del pass_back_counter
    flush_events()
    gc.collect()
    after_drop = _live_handle_count()

    print(
        {
            "direct_calls": direct,
            "pass_back_error": error_text,
            "handles": {
                "baseline": baseline,
                "after_direct_return": after_direct_return,
                "after_direct_drop": after_direct_drop,
                "after_failed_pass_back_drop": after_drop,
            },
        }
    )


if __name__ == "__main__":
    main()
