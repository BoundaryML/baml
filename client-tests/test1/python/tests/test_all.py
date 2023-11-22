import pytest
from baml_client import baml

from baml_client.baml_types import (
    Conversation,
    ProposedMessage,
    IMaybePolishText,
    ITextPolisher,
)
from baml_client.testing import baml_test


async def some_traced_fn():
    pass


from baml_client import baml
from baml_client.baml_types import IBlah
from baml_lib._impl.deserializer import Deserializer


@baml.Blah.test
async def test_mytest(BlahImpl: IBlah):
    deserializer = Deserializer[str](str)
    param = deserializer.from_string("""asdfasdf""")
    await BlahImpl(param)


@baml_test
@pytest.mark.asyncio
async def test_logic() -> None:
    result = await baml.MaybePolishText.get_impl("v1").run(
        ProposedMessage(thread=Conversation(thread=[]), generated_response="test")
    )
    print(result)


@baml.MaybePolishText.test
class TestThingies:
    def test_with_a_class(self, MaybePolishTextImpl: IMaybePolishText):
        pass


class TestMultiple:
    @baml.MaybePolishText.test
    def test_with_class2(self, MaybePolishTextImpl: IMaybePolishText):
        pass

    @baml.TextPolisher.test
    def test_with_class3(self, TextPolisherImpl: ITextPolisher):
        pass


def test_without_fixture3():
    pass
