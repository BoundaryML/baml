"""Host results and arguments share inbound checking, including sync calls."""

import asyncio

import pytest
from pydantic import BaseModel

from baml_bridge import BamlRuntime, call_function, call_function_sync
from baml_bridge.errors import BamlPanic
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


SOURCE = """
class Item { value: float, }
enum Color { Red Green }
enum Mood { Happy Sad }
function EnumUnion(value: Color | Mood) -> int {
    match (value) { let m: Mood => 7, _ => 3 }
}
function HostEnumUnion(f: () -> (Color | Mood)) -> int { EnumUnion(f()) }
function Items(value: Item[]) -> float { value[0].value / 2.0 }
function ItemMap(value: map<string, Item>) -> float { value["a"].value / 2.0 }
function HostItems(f: () -> Item[]) -> float { f()[0].value / 2.0 }
function HostItemMap(f: () -> map<string, Item>) -> float { f()["a"].value / 2.0 }
"""


class BoundaryDiagnostic(BaseModel):
    """Decode engine boundary diagnostics without requiring a generated SDK."""

    message: str
    class_name: str | None = None
    language: str | None = None


@pytest.fixture(scope="module")
def runtime():
    """Initialize the shared fixture runtime and restore its diagnostic typemap."""
    previous = get_type_map()
    set_type_map(
        BamlTypeMap.from_lazy_entries(
            classes={
                name: (__name__, "BoundaryDiagnostic")
                for name in [
                    "baml.errors.TypeMismatch",
                    "baml.panics.HostContractViolation",
                ]
            },
            enums={},
            type_aliases={},
        )
    )
    try:
        yield BamlRuntime.initialize_runtime(".", {"main.baml": SOURCE})
    finally:
        set_type_map(previous)


@pytest.mark.parametrize("sync_call", [False, True])
@pytest.mark.parametrize("async_callback", [False, True])
@pytest.mark.parametrize("name", ["Items", "ItemMap"])
@pytest.mark.parametrize("field", [612, "bad", None])
@pytest.mark.asyncio
async def test_host_inbound_parity(runtime, sync_call, async_callback, name, field):
    """Compare argument and callback ingress for every supported call mode."""
    item = {} if field is None else {"value": field}
    value = [item] if name == "Items" else {"a": item}

    def callback():
        """Deliver the same payload through synchronous callback completion."""
        return value

    async def async_fn():
        """Deliver the payload after suspending the host callback."""
        await asyncio.sleep(0)
        return value

    async def invoke(function, args):
        """Exercise the selected outer call API without changing the payload."""
        if sync_call:
            return call_function_sync(runtime, function, args).result()
        return (await call_function(runtime, function, args)).result()

    if field == 612:
        assert await invoke(name, {"value": value}) == 306.0
        assert (
            await invoke(f"Host{name}", {"f": async_fn if async_callback else callback})
            == 306.0
        )
    else:
        detail = "Missing field.*value" if field is None else "string"
        with pytest.raises(TypeError, match=detail):
            await invoke(name, {"value": value})
        with pytest.raises(BamlPanic) as caught:
            await invoke(f"Host{name}", {"f": async_fn if async_callback else callback})
        assert "HostContractViolation" in str(caught.value)
        assert ("Missing field" if field is None else "string") in str(caught.value)


@pytest.mark.parametrize("sync_call", [False, True])
@pytest.mark.parametrize("async_callback", [False, True])
@pytest.mark.parametrize("member, expected", [("Sad", 7), ("Red", 3), ("Missing", None)])
@pytest.mark.asyncio
async def test_enum_union_membership(runtime, sync_call, async_callback, member, expected):
    """Choose only enums that declare the string, for arguments and callbacks."""
    def callback():
        """Return a string representation of an enum member."""
        return member

    async def async_fn():
        """Return the same representation after suspension."""
        await asyncio.sleep(0)
        return member

    async def invoke(function, args):
        """Exercise both supported outer call APIs."""
        if sync_call:
            return call_function_sync(runtime, function, args).result()
        return (await call_function(runtime, function, args)).result()

    if expected is not None:
        assert await invoke("EnumUnion", {"value": member}) == expected
        assert await invoke("HostEnumUnion", {"f": async_fn if async_callback else callback}) == expected
    else:
        with pytest.raises(TypeError):
            await invoke("EnumUnion", {"value": member})
        with pytest.raises(BamlPanic, match="HostContractViolation"):
            await invoke("HostEnumUnion", {"f": async_fn if async_callback else callback})
