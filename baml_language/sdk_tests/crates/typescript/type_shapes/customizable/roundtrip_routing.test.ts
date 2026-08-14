// Roundtrip coverage for the cross-namespace routing suite — ported from
// test_routing.py. The baml.http.Response-typed round trips are covered in
// roundtrip_streams.test.ts (they need an engine-minted handle).
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { Foo, make_foo, round_trip_foo } from "./baml_sdk/index.js";
import { round_trip_deep_thing_from_a } from "./baml_sdk/a/index.js";
import { Thing, round_trip_thing_from_ab, round_trip_root_foo_from_ab } from "./baml_sdk/a/b/index.js";
import {
  Resume,
  round_trip_resume,
  round_trip_root_foo,
  round_trip_deep_thing_from_lorem,
} from "./baml_sdk/lorem/index.js";
import { round_trip_lorem_resume_from_ipsum } from "./baml_sdk/ipsum/index.js";

describe("roundtrip routing", () => {
  it("routing_make_foo", () => expect(make_foo(3).v).toBe(3));
  it("routing_round_trip_foo", () => {
    const f = new Foo({ v: 10 });
    expect(round_trip_foo(f)).toEqual(f);
  });
  it("routing_round_trip_thing_from_ab", () => {
    const t = new Thing({ v: 1 });
    expect(round_trip_thing_from_ab(t)).toEqual(t);
  });
  it("routing_round_trip_root_foo_from_ab", () => {
    const f = new Foo({ v: 2 });
    expect(round_trip_root_foo_from_ab(f)).toEqual(f);
  });
  it("routing_round_trip_deep_thing_from_a", () => {
    const t = new Thing({ v: 4 });
    expect(round_trip_deep_thing_from_a(t)).toEqual(t);
  });
  it("routing_round_trip_deep_thing_from_lorem", () => {
    const t = new Thing({ v: 5 });
    expect(round_trip_deep_thing_from_lorem(t)).toEqual(t);
  });
  it("routing_round_trip_resume", () => {
    const r = new Resume({ name: "ada", email: null });
    expect(round_trip_resume(r)).toEqual(r);
  });
  it("routing_round_trip_root_foo", () => {
    const f = new Foo({ v: 6 });
    expect(round_trip_root_foo(f)).toEqual(f);
  });
  it("routing_round_trip_lorem_resume_from_ipsum", () => {
    const r = new Resume({ name: "grace", email: "g@x.com" });
    expect(round_trip_lorem_resume_from_ipsum(r)).toEqual(r);
  });
});
