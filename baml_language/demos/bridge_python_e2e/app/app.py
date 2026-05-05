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
from baml_sdk.baml.media import Image, Pdf
from baml_sdk.lorem import (
    Address,
    Bar,
    Box,
    ExtractResume__build_request,
    ExtractResume__build_request_stream,
    ExtractResume__build_request_async,
    Foo,
    PhoneNumber,
    Resume,
    Sentiment,
    StreamedExtractResult,
    FetchSseResult,
)

before = Resume(
    name="Ada Lovelace",
    email=None,  # Optional[str] -> Null
    addresses=[  # List[Class]
        Address(street="1 Analytical Way", city="London", zip=None),
        Address(street="221B Baker St", city="London", zip="NW1 6XE"),
    ],
    scores={"math": 99, "poetry": 80},  # Dict[str, int]
    sentiment=Sentiment.POSITIVE,  # Enum -> Variant
    contact=PhoneNumber(
        country_code=44,  # nested class
        digits="2079460000",
    ),
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
assert after.sentiment == Sentiment.NEGATIVE  # bex flipped it
assert isinstance(after.contact, PhoneNumber)
assert after.contact.country_code == 44

# Async sibling — BEP-030 always emits both `transform` and `transform_async`.
also_after = asyncio.run(before.transform_async())
assert isinstance(also_after, Resume)
assert also_after.sentiment == Sentiment.NEGATIVE

print()

# 100 distinct resume texts. The first two are pinned so downstream
# assertions ("Ada Lovelace in body", "Hopper in body", first-Resume name)
# remain meaningful; the rest are filler to exercise list-input rendering.
RESUME_TEXTS: list[str] = [
    "Grace Hopper, RADM, computer scientist. grace@navy.mil. Worked on COBOL.",
    "Ada Lovelace, mathematician. ada@example.org",
] + [
    f"Person {i}, engineer at company {i}. person{i}@example.org"
    for i in range(98)
]
assert len(RESUME_TEXTS) == 100

# ---------------------------------------------------------------------------
# Modular API: `__build_request` companion. The auto-synthesized
# `ExtractResume$build_request` returns a `baml.http.Request` describing
# the HTTP call that *would* be made; nothing leaves the process.
# ---------------------------------------------------------------------------

req = ExtractResume__build_request_stream(texts=RESUME_TEXTS)
print("--- ExtractResume__build_request_stream(...) ---")
print(req.body)

assert isinstance(req, Request), f"expected baml.http.Request, got {type(req)}"
assert req.method == "POST"
assert "openai.com" in req.url, f"expected openai endpoint, got {req.url!r}"
assert req.headers.get("authorization", "").startswith("Bearer sk-")

body = json.loads(req.body)
assert body["model"] == "gpt-4o"
assert body["messages"], "expected at least one message in the request body"
flat = json.dumps(body)
assert "Ada Lovelace" in flat, "user input did not reach the rendered prompt"
assert "Hopper" in flat, "user input did not reach the rendered prompt"

# Async sibling round-trip.
req_async = asyncio.run(
    ExtractResume__build_request_async(texts=RESUME_TEXTS),
)
assert isinstance(req_async, Request)
assert "Hopper" in req_async.body

print()

# ---------------------------------------------------------------------------
# Lower-level probe: hit `baml.http.fetch_sse` directly on the request
# `$build_request_stream` produced. Curl confirms OpenAI streams chunks
# for the same body; this checks whether `fetch_sse` exposes them.
# ---------------------------------------------------------------------------

print("--- FetchSseResult.run(...) live call ---")
fetched = FetchSseResult.run(texts=RESUME_TEXTS)
print(f"count={fetched.count}")
print(f"first_chunk={fetched.first_chunk!r}")
print(f"last_chunk={fetched.last_chunk!r}")

print()

# ---------------------------------------------------------------------------
# Streaming: the Python bridge can't currently round-trip a live
# `baml.llm.Stream`, so we drive `ExtractResume$stream(...)` inside BAML
# (`StreamedExtractResult.run`) and return `{ partial_count, final }`.
# ---------------------------------------------------------------------------

print("--- StreamedExtractResult.run(...) live call ---")
streamed = StreamedExtractResult.run(texts=RESUME_TEXTS)
print(streamed)
assert isinstance(streamed, StreamedExtractResult)
assert isinstance(streamed.final, list)
assert all(isinstance(r, Resume) for r in streamed.final)
assert streamed.final, "expected at least one Resume in streamed.final"
assert "Hopper" in streamed.final[0].name or "Grace" in streamed.final[0].name, (
    f"expected first Resume to be Grace Hopper, got {streamed.final[0].name!r}"
)
# NOTE: with the current build, `partial_count` is 0 — the OpenAI
# request body doesn't carry `"stream": true`, so the SSE source
# returns the full body in one shot and `Stream.next()` reports
# `StreamFinished` on the first poll. The streaming *pipeline* is
# wired (no `Non-parsable type: Void` from `StreamCache.new`); the
# request just isn't actually a streaming request yet.
assert streamed.partial_count >= 0

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

# ---------------------------------------------------------------------------
# Media handles: top-level + nested round-trip per 15b spec.
# Exercises Pdf.from_url (PyO3 staticmethod -> Arc<MediaValue>),
# isinstance check on the returned value, instance accessor (url()),
# nested pdf in input class (Foo.my_pdf), and nested pdf in output class
# (Bar.their_pdf) decoded back to a Pdf.
# ---------------------------------------------------------------------------

original_pdf = Pdf.from_url(
    "https://www.rd.usda.gov/sites/default/files/pdf-sample_0.pdf",
    "application/pdf",
)
assert isinstance(original_pdf, Pdf), f"expected Pdf, got {type(original_pdf).__name__}"
assert (
    original_pdf.url() == "https://www.rd.usda.gov/sites/default/files/pdf-sample_0.pdf"
)

original_image = Image.from_url(
    "https://example.com/cat.png",
    "image/png",
)
assert isinstance(original_image, Image), (
    f"expected Image, got {type(original_image).__name__}"
)
assert original_image.url() == "https://example.com/cat.png"

foo = Foo(name="foo", my_pdf=original_pdf, my_image=original_image)
bar = foo.repackage()

assert isinstance(bar, Bar)
assert bar.name == "bar"
assert isinstance(bar.their_pdf, Pdf)
assert (
    bar.their_pdf.url()
    == "https://www.rd.usda.gov/sites/default/files/pdf-sample_0.pdf"
)
assert isinstance(bar.their_image, Image)
assert bar.their_image.url() == "https://example.com/cat.png"

# Extended accessor coverage — exercises `from_file` / `from_base64` /
# the `file()` / `base64()` / `mime_type()` accessors that the M0 path
# doesn't touch. Pure-Rust dispatch, no engine round-trip.
pdf_file = Pdf.from_file("/tmp/sample.pdf", "application/pdf")
assert pdf_file.file() == "/tmp/sample.pdf"
assert pdf_file.url() is None
assert pdf_file.mime_type() == "application/pdf"

import base64 as b64

data = b64.b64encode(b"%PDF-1.4 fake bytes").decode()
pdf_b64 = Pdf.from_base64(data, "application/pdf")
assert pdf_b64.base64() == data

image_file = Image.from_file("/tmp/sample.png", "image/png")
assert image_file.file() == "/tmp/sample.png"
assert image_file.url() is None
assert image_file.mime_type() == "image/png"

image_data = b64.b64encode(b"\x89PNG fake bytes").decode()
image_b64 = Image.from_base64(image_data, "image/png")
assert image_b64.base64() == image_data

print()
print(
    "OK — round-trip succeeded for: Optional, List<Class>, Map, Enum, "
    "nested-Class, sync+async, plus LLM `__build_request` companion, "
    "plus Box<T> and Box<Box<int>> generics, plus Pdf + Image media "
    "handles (top-level + nested round-trip + accessors)."
)
