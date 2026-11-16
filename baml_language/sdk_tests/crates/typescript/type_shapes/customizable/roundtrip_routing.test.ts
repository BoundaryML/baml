// Roundtrip coverage for the cross-namespace routing suite — ported from
// test_routing.py. The baml.http.Response arms need an engine-minted handle;
// the handle round trip is covered by roundtrip_handles.test.ts.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { Foo, make_foo_sync, round_trip_foo_sync } from "./baml_sdk/index.js";
import { round_trip_deep_thing_from_a_sync } from "./baml_sdk/a/index.js";
import { Thing, round_trip_thing_from_ab_sync, round_trip_root_foo_from_ab_sync } from "./baml_sdk/a/b/index.js";
import {
  Resume,
  round_trip_resume_sync,
  round_trip_root_foo_sync,
  round_trip_deep_thing_from_lorem_sync,
  round_trip_resume_or_http_response_sync,
} from "./baml_sdk/lorem/index.js";
import { round_trip_lorem_resume_from_ipsum_sync } from "./baml_sdk/ipsum/index.js";

describe("roundtrip routing", () => {
  it("routing_make_foo", () => expect(make_foo_sync(3).v).toBe(3));
  it("routing_round_trip_foo", () => {
    const f = new Foo({ v: 10 });
    expect(round_trip_foo_sync(f)).toEqual(f);
  });
  it("routing_round_trip_thing_from_ab", () => {
    const t = new Thing({ v: 1 });
    expect(round_trip_thing_from_ab_sync(t)).toEqual(t);
  });
  it("routing_round_trip_root_foo_from_ab", () => {
    const f = new Foo({ v: 2 });
    expect(round_trip_root_foo_from_ab_sync(f)).toEqual(f);
  });
  it("routing_round_trip_deep_thing_from_a", () => {
    const t = new Thing({ v: 4 });
    expect(round_trip_deep_thing_from_a_sync(t)).toEqual(t);
  });
  it("routing_round_trip_deep_thing_from_lorem", () => {
    const t = new Thing({ v: 5 });
    expect(round_trip_deep_thing_from_lorem_sync(t)).toEqual(t);
  });
  it("routing_round_trip_resume", () => {
    const r = new Resume({ name: "ada", email: null });
    expect(round_trip_resume_sync(r)).toEqual(r);
  });
  it("routing_round_trip_root_foo", () => {
    const f = new Foo({ v: 6 });
    expect(round_trip_root_foo_sync(f)).toEqual(f);
  });
  it("routing_round_trip_lorem_resume_from_ipsum", () => {
    const r = new Resume({ name: "grace", email: "g@x.com" });
    expect(round_trip_lorem_resume_from_ipsum_sync(r)).toEqual(r);
  });
  it("routing_round_trip_resume_or_http_response", () => {
    const r = new Resume({ name: "lovelace", email: "a@x.com" });
    expect(round_trip_resume_or_http_response_sync(r)).toEqual(r);
  });
});
