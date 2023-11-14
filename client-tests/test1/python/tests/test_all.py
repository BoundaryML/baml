import pytest
from baml_client import baml

from baml_client.baml_types import (
    Conversation,
    ProposedMessage,
)
from baml_client.testing import baml_test


async def some_traced_fn():
    pass


from baml_client import baml_init
from typing import Sequence
from baml_client.tracing import LogSchema


baml_init(
    before_message_export_hook=message_export,
    message_transformer_hook=message_transformer,
)


@baml_test
@pytest.mark.asyncio
async def test_logic() -> None:
    result = await baml.MaybePolishText.get_impl("v1").run(
        ProposedMessage(thread=Conversation(thread=[]), generated_response="test")
    )
    print(result)


# @baml.MaybePolishText.test
# class TestThingies:
#     def test_with_a_class(self, MaybePolishTextImpl: IMaybePolishText):
#         pass


# class TestMultiple:
#     @baml.MaybePolishText.test
#     def test_with_class2(self, MaybePolishTextImpl: IMaybePolishText):
#         pass

#     @baml.TextPolisher.test
#     def test_with_class3(self, TextPolisherImpl: ITextPolisher):
#         pass


def test_without_fixture3():
    pass
