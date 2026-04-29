"""Smoke demo: write a typed Resume, send it through BAML, get a typed
Resume back. Exits 0 on success, prints the round-tripped value, and
asserts each non-trivial type conversion held.

Also calls the auto-synthesized `ExtractResume__build_request` companion
to prove the modular API path runs end-to-end without an actual LLM
call (the stub client uses `api_key "sk-test"`).
"""

import asyncio
import json

from baml_sdk.baml.http import Request
from baml_sdk.lorem import (
    Address,
    Box,
    ExtractResume__build_request,
    ExtractResume__build_request_async,
    PhoneNumber,
    Resume,
    Sentiment,
)

before = Resume(
    name="Ada Lovelace",
    email=None,                                    # Optional[str] -> Null
    addresses=[                                    # List[Class]
        Address(street="1 Analytical Way", city="London", zip=None),
        Address(street="221B Baker St",    city="London", zip="NW1 6XE"),
    ],
    scores={"math": 99, "poetry": 80},             # Dict[str, int]
    sentiment=Sentiment.POSITIVE,                  # Enum -> Variant
    contact=PhoneNumber(country_code=44,           # nested class
                        digits="2079460000"),
)

print("--- before ---")
print(before)

after = before.transform()

print("--- after (sync) ---")
print(after)

assert isinstance(after, Resume)
assert after.name == "Ada Lovelace"
assert after.email is None
assert [a.zip for a in after.addresses] == [None, "NW1 6XE"]
assert after.scores == {"math": 99, "poetry": 80}
assert after.sentiment == Sentiment.NEGATIVE       # bex flipped it
assert isinstance(after.contact, PhoneNumber)
assert after.contact.country_code == 44

# Async sibling — BEP-030 always emits both `transform` and `transform_async`.
also_after = asyncio.run(before.transform_async())
assert isinstance(also_after, Resume)
assert also_after.sentiment == Sentiment.NEGATIVE

print()

# ---------------------------------------------------------------------------
# Modular API: `__build_request` companion. The auto-synthesized
# `ExtractResume$build_request` returns a `baml.http.Request` describing
# the HTTP call that *would* be made; nothing leaves the process.
# ---------------------------------------------------------------------------

req = ExtractResume__build_request(
    text="Ada Lovelace, mathematician. ada@example.org",
)
print("--- ExtractResume__build_request(...) ---")
print(req)

assert isinstance(req, Request), f"expected baml.http.Request, got {type(req)}"
assert req.method == "POST"
assert "openai.com" in req.url, f"expected openai endpoint, got {req.url!r}"
assert req.headers.get("authorization", "").startswith("Bearer sk-test")

body = json.loads(req.body)
assert body["model"] == "gpt-4o"
assert body["messages"], "expected at least one message in the request body"
# The interpolated `{{ text }}` should appear verbatim in the rendered prompt.
flat = json.dumps(body)
assert "Ada Lovelace" in flat, "user input did not reach the rendered prompt"

# Async sibling round-trip.
req_async = asyncio.run(
    ExtractResume__build_request_async(text="Hopper, RADM."),
)
assert isinstance(req_async, Request)
assert "Hopper" in req_async.body

print()

# ---------------------------------------------------------------------------
# Generics: 13a generates `class Box(BaseModel, Generic[T])`; 13b
# round-trips a parameterized instance through the engine. The
# inbound encoder uses the *base* FQN ("user.lorem.Box") regardless of
# parameterization; the outbound decoder calls `_parameterize` so the
# returned instance lines up with the static-checker annotation.
# ---------------------------------------------------------------------------

box_int = Box[int](item=5)
print("--- Box[int] before ---")
print(box_int)

box_int_after = box_int.repackage()
print("--- Box[int] after ---")
print(box_int_after)

assert isinstance(box_int_after, Box)
assert box_int_after.item == 5

# Generic-of-generic: Box[Box[int]]. Tests that the inbound encoder
# walks the nested Pydantic model (the inner Box[int] hits the
# BaseModel branch via `_set_inbound_value` recursion) and that the
# outbound decoder rebuilds the inner Box before validating the outer.
inner = Box[int](item=42)
outer = Box[Box[int]](item=inner)
print("--- Box[Box[int]] before ---")
print(outer)

outer_after = outer.repackage()
print("--- Box[Box[int]] after ---")
print(outer_after)

assert isinstance(outer_after, Box)
assert isinstance(outer_after.item, Box), (
    f"expected nested Box, got {type(outer_after.item).__name__}: {outer_after.item!r}"
)
assert outer_after.item.item == 42

# Async sibling — same shape, just to prove the async factory threads
# generics through too.
outer_after_async = asyncio.run(outer.repackage_async())
assert isinstance(outer_after_async, Box)
assert isinstance(outer_after_async.item, Box)
assert outer_after_async.item.item == 42

print()
print("OK — round-trip succeeded for: Optional, List<Class>, Map, Enum, "
      "nested-Class, sync+async, plus LLM `__build_request` companion, "
      "plus Box<T> and Box<Box<int>> generics.")
