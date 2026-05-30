// Roundtrip coverage for the lorem stream-type / stdlib-routing suite —
// ported from test_streams.py. The baml.http.Response-backed cases are
// skipped (the engine must mint the `_body` handle; not host-constructible).
import "./baml_sdk";
import { describe, it, expect } from "@jest/globals";
import {
  Box,
  Resume,
  round_trip_resume_stream,
  round_trip_root_foo_stream,
  round_trip_baml_http_response_stream, // eslint-disable-line @typescript-eslint/no-unused-vars
  round_trip_box_of_resume_stream,
  round_trip_list_of_http_response, // eslint-disable-line @typescript-eslint/no-unused-vars
  round_trip_resume_or_http_response,
  round_trip_resume_or_resume_stream,
} from "./baml_sdk/lorem";
import { Resume as StreamResume } from "./baml_sdk/stream_types/lorem";
import { Foo as StreamFoo } from "./baml_sdk/stream_types";

void round_trip_baml_http_response_stream; // import-only: handle-backed
void round_trip_list_of_http_response; // import-only: handle-backed

const HANDLE_REASON =
  "baml.http.Response carries a _body handle that only the engine can mint";

describe("roundtrip streams", () => {
  it("round_trip_resume_stream", () => {
    const r = new StreamResume({ name: "ada", email: null });
    expect(round_trip_resume_stream(r)).toEqual(r);
  });
  it("round_trip_root_foo_stream", () => {
    const f = new StreamFoo({ v: 3 });
    expect(round_trip_root_foo_stream(f)).toEqual(f);
  });
  it("round_trip_box_of_resume_stream", () => {
    const b = new Box({ v: new StreamResume({ name: "grace", email: null }) });
    expect(round_trip_box_of_resume_stream(b)).toEqual(b);
  });
  it("round_trip_resume_or_resume_stream", () => {
    // Union arm `Resume` (the non-stream side) is host-constructible.
    const r = new Resume({ name: "hopper", email: null });
    expect(round_trip_resume_or_resume_stream(r)).toEqual(r);
  });
  it("round_trip_resume_or_http_response", () => {
    const r = new Resume({ name: "lovelace", email: "a@x.com" });
    expect(round_trip_resume_or_http_response(r)).toEqual(r);
  });
  it.skip(`round_trip_baml_http_response_stream — ${HANDLE_REASON}`, () => {});
  it.skip(`round_trip_list_of_http_response — ${HANDLE_REASON}`, () => {});
});
