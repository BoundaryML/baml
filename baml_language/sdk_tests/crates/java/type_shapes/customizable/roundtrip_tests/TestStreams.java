// Roundtrip coverage for the `lorem` stream-type / stdlib-routing suite.
//
// These are the riskiest shapes in the fixture: `$stream` companion types
// (`Resume$stream`, `Foo$stream`) are normally engine-internal *partial*
// values. Whether a host-constructed `$stream` companion instance can be
// encoded and round-tripped through a `$stream`-typed parameter is what
// these tests probe.
//
// `baml.http.Response`-backed parameters (bare, in a list, or as a
// `$stream`) can't be driven from pure Java either — the underlying handle
// is engine-minted — so they're omitted here; the handle round-trip is
// covered by TestHandles.java instead.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_streams.py — same test names, cases, inputs, assertions.
//
// java-port note: GAP B decided 2026-07-17 (Option B, TS-aligned). Stream
// partial-model companions keep the in-package `$`-preserved naming
// (`baml_sdk.lorem.Resume$stream`, `baml_sdk.Foo$stream`), matching TS.
// Python's `stream_types.*` sub-package layout is Python-legacy and is NOT
// mirrored here. `$` is a legal Java identifier character, so the companion
// types are referenced by their fully qualified `Name$stream` symbol below.
// (Python aliases `stream_types.lorem.Resume` -> `StreamResume` and
// `stream_types.Foo` -> `StreamFoo` via `import ... as ...` only because its
// legacy layout collides with `lorem.Resume` / root `Foo`; the `$stream`
// naming has no such collision.)
//
// java-port note: `Resume | Resume$stream` -> `Union2<Resume,
// Resume$stream>` and `Resume | baml.http.Response` -> `Union2<Resume,
// baml.http.Response>`; see TestUnions.java for the general generic-family
// shape this port assumes. Arms are positional (Arm0 = `Resume`, Arm1 = the
// stream / http-response arm) in BAML declaration order, so the two
// `Resume`-derived arms no longer collide by name. Only the
// host-constructible `Resume` arm (Arm0) is exercised here.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_bridge.Union2;
import baml_sdk.lorem.Box;
import baml_sdk.lorem.Fns;
import baml_sdk.lorem.Resume;
import org.junit.jupiter.api.Test;

class TestStreams {

    @Test
    void test_streams_round_trip_resume_stream() {
        baml_sdk.lorem.Resume$stream r =
                new baml_sdk.lorem.Resume$stream("ada", null);
        assertEquals(r, Fns.round_trip_resume_stream(r));
    }

    @Test
    void test_streams_round_trip_root_foo_stream() {
        baml_sdk.Foo$stream f = new baml_sdk.Foo$stream(3L);
        assertEquals(f, Fns.round_trip_root_foo_stream(f));
    }

    @Test
    void test_streams_round_trip_box_of_resume_stream() {
        Box<baml_sdk.lorem.Resume$stream> b =
                new Box<>(new baml_sdk.lorem.Resume$stream("grace", null));
        assertEquals(b, Fns.round_trip_box_of_resume_stream(b));
    }

    @Test
    void test_streams_round_trip_resume_or_resume_stream() {
        // Union arm `Resume` (the non-stream side) is host-constructible.
        Resume r = new Resume("hopper", null);
        Object result =
                Fns.round_trip_resume_or_resume_stream(
                        new Union2.Arm0<Resume, baml_sdk.lorem.Resume$stream>(r));
        assertEquals(r, ((Union2.Arm0<?, ?>) result).value());
    }

    @Test
    void test_streams_round_trip_resume_or_http_response() {
        // Pass the `Resume` arm; the `baml.http.Response` arm isn't
        // host-constructible.
        Resume r = new Resume("lovelace", "a@x.com");
        Object result =
                Fns.round_trip_resume_or_http_response(
                        new Union2.Arm0<Resume, baml_sdk.baml.http.Response>(r));
        assertEquals(r, ((Union2.Arm0<?, ?>) result).value());
    }
}
