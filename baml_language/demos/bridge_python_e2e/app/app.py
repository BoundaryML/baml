"""Smoke demo: write a typed Resume, send it through BAML, get a typed
Resume back. Exits 0 on success, prints the round-tripped value, and
asserts each non-trivial type conversion held.

Also calls the auto-synthesized `ExtractResume__build_request` companion
to prove the modular API path runs end-to-end without an actual LLM
call (the stub client uses `api_key "sk-test"`).
"""

import asyncio
import json

from baml_sdk import (
    Address,
    ExtractResume__build_request,
    ExtractResume__build_request_async,
    PhoneNumber,
    Resume,
    Sentiment,
)
from baml_sdk.baml.http import Request

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
print("OK — round-trip succeeded for: Optional, List<Class>, Map, Enum, "
      "nested-Class, sync+async, plus LLM `__build_request` companion.")
