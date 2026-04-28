"""Smoke demo: write a typed Resume, send it through BAML, get a typed
Resume back. Exits 0 on success, prints the round-tripped value, and
asserts each non-trivial type conversion held."""

import asyncio

from baml_sdk.lorem import Address, PhoneNumber, Resume, Sentiment

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
print("OK — round-trip succeeded for: Optional, List<Class>, Map, Enum, "
      "nested-Class, sync+async.")
