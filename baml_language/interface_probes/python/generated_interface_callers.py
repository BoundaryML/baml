"""Exercise an actual Python SDK generated from sdk_tests/fixtures/interfaces.

Run with the generated parent directory on PYTHONPATH and the SDK's Python
environment. This supplements shared tests; it is not all-language parity.
"""

import asyncio

import baml_sdk
from baml_bridge import proto
from baml_bridge._interface import BamlInterfaceRef
from typing_extensions import Never


async def main() -> None:
    echo = await baml_sdk.make_echo_async()
    assert await echo.echo("Ada", _types={"T": str}) == "Ada"
    definition = proto.BamlType._from_python(str)
    assert await echo.echo("Grace", _types={"T": definition}) == "Grace"

    counter = await baml_sdk.make_counter_async(2)
    returned = await echo.echo(counter, _types={"T": baml_sdk.Counter})
    assert await returned.add(3) == 5
    assert await counter.current() == 5

    decoder = await baml_sdk.make_text_decoder_async()
    assert isinstance(decoder, baml_sdk.DecoderRef)
    assert proto.python_type_to_wire_ty(
        baml_sdk.Decoder[str, Never]
    ) == proto.python_type_to_wire_ty(baml_sdk.DecoderRef[str, Never])

    sources = [
        await baml_sdk.as_string_iterable_async(["Ada"]),
        await baml_sdk.as_character_iterable_async("A"),
    ]
    for source in sources:
        iterator = await source.iter()
        assert isinstance(iterator, BamlInterfaceRef)
        assert isinstance(await iterator.next(), str)
        assert isinstance(await iterator.next(), baml_sdk.baml.iter.Done)

    iterator = await baml_sdk.nullable_iterator_async([None, "Ada"])
    assert await iterator.next() is None
    assert await iterator.next() == "Ada"
    assert isinstance(await iterator.next(), baml_sdk.baml.iter.Done)
    print("Generated interface caller probes passed")


if __name__ == "__main__":
    asyncio.run(main())
