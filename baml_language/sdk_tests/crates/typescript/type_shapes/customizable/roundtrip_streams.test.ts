// Roundtrip coverage for the lorem stream-type / stdlib-routing suite —
// ported from test_streams.py. Host-constructible shapes only: the
// `baml.http.Response`-backed cases aren't host-mintable (the engine owns the
// `_body` handle), and that handle round-trip is covered by
// roundtrip_handles.test.ts instead.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Box,
  Resume,
  round_trip_resume_stream,
  round_trip_root_foo_stream,
  round_trip_box_of_resume_stream,
  round_trip_resume_or_http_response,
  round_trip_resume_or_resume_stream,
  // spec2: stream companions keep their `$stream` suffix and live beside
  // their base type in the same leaf — no `stream_types/` namespace.
  Resume$stream,
} from "./baml_sdk/lorem/index.js";
import { Foo$stream } from "./baml_sdk/index.js";

describe("roundtrip streams", () => {
  it("streams_round_trip_resume_stream", () => {
    const r = new Resume$stream({ name: "ada", email: null });
    expect(round_trip_resume_stream(r)).toEqual(r);
  });
  it("streams_round_trip_root_foo_stream", () => {
    const f = new Foo$stream({ v: 3 });
    expect(round_trip_root_foo_stream(f)).toEqual(f);
  });
  // `Box<...>` is a generic class passed directly as an argument. The TS
  // encoder now sends it as a FQN-tagged `class_value` with the value-level
  // class type-args channel (`class_ty`), so the engine decodes it to an
  // `Instance` and accepts it into the generic class slot. (No `$types` is
  // passed, so the arg lowers to the unknown/top wildcard.)
  it("streams_round_trip_box_of_resume_stream", () => {
    const b = new Box({ v: new Resume$stream({ name: "grace", email: null }) });
    expect(round_trip_box_of_resume_stream(b)).toEqual(b);
  });
  it("streams_round_trip_resume_or_resume_stream", () => {
    // Union arm `Resume` (the non-stream side) is host-constructible.
    const r = new Resume({ name: "hopper", email: null });
    expect(round_trip_resume_or_resume_stream(r)).toEqual(r);
  });
  it("streams_round_trip_resume_or_http_response", () => {
    const r = new Resume({ name: "lovelace", email: "a@x.com" });
    expect(round_trip_resume_or_http_response(r)).toEqual(r);
  });
});
