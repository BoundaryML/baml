import pytest
from ..baml_client import b
from ..baml_client.types import MaintainFieldOrder

# Order of fields in classes can be messed up because of HashMap in Rust.
# Use IndexMap everywhere.
@pytest.mark.asyncio
async def test_maintain_field_order():
    request = await b.request.UseMaintainFieldOrder(
        MaintainFieldOrder(a="1", b="2", c="3")
    )

    assert request.body.json() == {
        "model": "gpt-4o-mini",
        "messages": [
            {
                "role": "system",
                "content": [
                    {
                        "type": "text",
                        "text": '''Return this value back to me: {
    "a": "1",
    "b": "2",
    "c": "3",
}''',
                    }
                ],
            }
        ],
    }