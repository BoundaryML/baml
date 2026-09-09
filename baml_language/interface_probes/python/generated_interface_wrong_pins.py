"""Negative pyright probe: expect exactly two reportAssignmentType errors.

Use the SDK generated from sdk_tests/fixtures/interfaces as an extraPath.
Keep both mismatches: Error is invariant even without a Python throws type.
"""

from typing_extensions import Never

from baml_sdk import DecoderRef, make_text_decoder_async


async def wrong_pins() -> None:
    value = await make_text_decoder_async()
    wrong_output: DecoderRef[int, Never] = value
    wrong_error: DecoderRef[str, str] = value
    _ = wrong_output, wrong_error
