from baml_client import baml
from baml_client.baml_types import (
    IMaybePolishText,
    Conversation,
    ProposedMessage,
    ITextPolisher,
)
from baml_core.otel import trace, set_tags


async def some_traced_fn():
    pass


@baml.MaybePolishText.test  # parameterizes on impls of MaybePolishText
async def test_logic(MaybePolishTextImpl: IMaybePolishText) -> None:
    result = await MaybePolishTextImpl(
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
