import pytest
from ..baml_client import b
from ..baml_client.types import MaintainFieldOrder

# Order of fields in classes can be messed up because of HashMap/BTreeMap in Rust.
# Use IndexMap everywhere. Field names c, b, a are intentionally not alphabetical
# to catch ordering bugs (alphabetical would be a, b, c).
@pytest.mark.asyncio
async def test_maintain_field_order():
    request = await b.request.UseMaintainFieldOrder(
        MaintainFieldOrder(c="1", b="2", a="3")
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
    "c": "1",
    "b": "2",
    "a": "3",
}''',
                    }
                ],
            }
        ],
    }