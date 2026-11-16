// Roundtrip coverage for the symbol-collision suite — ported from
// test_symbol_collisions.py. Three distinct `Bar` classes at different
// namespace depths plus the consumers (`Ipsum`, `Deep`).
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { make_foo_bar_sync, round_trip_foo_bar_sync } from "./baml_sdk/symbol_collisions/foo/index.js";
import { make_fizz_foo_bar_sync, round_trip_fizz_foo_bar_sync } from "./baml_sdk/symbol_collisions/fizz/foo/index.js";
import {
  make_fizz_buzz_foo_bar_sync,
  round_trip_fizz_buzz_foo_bar_sync,
} from "./baml_sdk/symbol_collisions/fizz/buzz/foo/index.js";
import { make_ipsum_sync, round_trip_ipsum_sync } from "./baml_sdk/symbol_collisions/lorem/index.js";
import { make_deep_sync, round_trip_deep_sync } from "./baml_sdk/symbol_collisions/a/b/c/d/index.js";

describe("roundtrip symbol_collisions", () => {
  it("symbol_collisions_round_trip_foo_bar", () => {
    const bar = make_foo_bar_sync("hi", 2);
    expect(round_trip_foo_bar_sync(bar)).toEqual(bar);
  });
  it("symbol_collisions_round_trip_fizz_foo_bar", () => {
    const bar = make_fizz_foo_bar_sync("t", 1.5);
    expect(round_trip_fizz_foo_bar_sync(bar)).toEqual(bar);
  });
  it("symbol_collisions_round_trip_fizz_buzz_foo_bar", () => {
    const bar = make_fizz_buzz_foo_bar_sync("f", 2.5, true);
    expect(round_trip_fizz_buzz_foo_bar_sync(bar)).toEqual(bar);
  });
  it("symbol_collisions_round_trip_ipsum", () => {
    const ipsum = make_ipsum_sync(
      make_foo_bar_sync("a", 1),
      make_fizz_foo_bar_sync("b", 2.0),
      make_fizz_buzz_foo_bar_sync("c", 3.0, false),
    );
    expect(round_trip_ipsum_sync(ipsum)).toEqual(ipsum);
  });
  it("symbol_collisions_round_trip_deep", () => {
    const ipsum = make_ipsum_sync(
      make_foo_bar_sync("a", 1),
      make_fizz_foo_bar_sync("b", 2.0),
      make_fizz_buzz_foo_bar_sync("c", 3.0, false),
    );
    const deep = make_deep_sync(
      make_foo_bar_sync("h", 9),
      make_fizz_foo_bar_sync("th", 4.0),
      make_fizz_buzz_foo_bar_sync("fu", 5.0, true),
      ipsum,
    );
    expect(round_trip_deep_sync(deep)).toEqual(deep);
  });
});
