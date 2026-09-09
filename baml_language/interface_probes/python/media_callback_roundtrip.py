"""Probe concrete image transfer through a real BAML -> Python callback.

Run from baml_language/sdks/python after building the editable bridge:

    uv run python ../../interface_probes/python/media_callback_roundtrip.py
"""

from __future__ import annotations

import gc

from baml_bridge import BamlRuntime, call_function_sync, flush_events
from baml_bridge.baml_py import BamlImage, _live_handle_count


SOURCE = """
function Roundtrip(callback: (image) -> image, value: image) -> image {
    callback(value)
}

function CreatedRoundtrip(callback: (image) -> image) -> image {
    callback(baml.media.Image.from_base64("YmFtbC1jcmVhdGVk", "image/webp"))
}
"""


def main() -> None:
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    baseline = _live_handle_count()
    original = BamlImage.from_base64("aW1hZ2U=", mime_type="image/png")
    after_original = _live_handle_count()
    seen: list[tuple[str, str, str | None, bool]] = []

    def callback(value: BamlImage) -> BamlImage:
        seen.append(
            (type(value).__name__, value.base64(), value.mime_type(), value is original)
        )
        return value

    returned = call_function_sync(
        runtime,
        "Roundtrip",
        {"callback": callback, "value": original},
    ).result()
    after_call = _live_handle_count()

    assert seen == [("BamlImage", "aW1hZ2U=", "image/png", False)]
    assert isinstance(returned, BamlImage)
    assert returned.base64() == "aW1hZ2U="
    assert returned.mime_type() == "image/png"
    assert returned is not original

    del returned
    flush_events()
    gc.collect()
    after_host_result_drop = _live_handle_count()

    assert after_original == baseline + 1
    assert after_call == baseline + 2
    assert after_host_result_drop == baseline + 1

    baml_seen: list[tuple[str, str, str | None]] = []

    def baml_callback(value: BamlImage) -> BamlImage:
        baml_seen.append((type(value).__name__, value.base64(), value.mime_type()))
        return value

    baml_created = call_function_sync(
        runtime,
        "CreatedRoundtrip",
        {"callback": baml_callback},
    ).result()
    after_baml_call = _live_handle_count()
    assert baml_seen == [("BamlImage", "YmFtbC1jcmVhdGVk", "image/webp")]
    assert isinstance(baml_created, BamlImage)
    assert baml_created.base64() == "YmFtbC1jcmVhdGVk"
    assert baml_created.mime_type() == "image/webp"

    del baml_created
    flush_events()
    gc.collect()
    after_baml_result_drop = _live_handle_count()
    assert after_baml_call == baseline + 2
    assert after_baml_result_drop == baseline + 1
    print(
        {
            "host_created_seen": seen,
            "baml_created_seen": baml_seen,
            "handles": {
                "baseline": baseline,
                "after_original": after_original,
                "after_host_call": after_call,
                "after_host_result_drop": after_host_result_drop,
                "after_baml_call": after_baml_call,
                "after_baml_result_drop": after_baml_result_drop,
            },
        }
    )


if __name__ == "__main__":
    main()
