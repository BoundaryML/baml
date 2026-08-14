// Roundtrip coverage for the symbol-collision suite — ported from
// test_symbol_collisions.py. Three distinct `Bar` classes at different
// namespace depths plus the consumers (`Ipsum`, `Deep`).
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { make_foo_bar, round_trip_foo_bar } from "./baml_sdk/symbol_collisions/foo/index.js";
import { make_fizz_foo_bar, round_trip_fizz_foo_bar } from "./baml_sdk/symbol_collisions/fizz/foo/index.js";
import {
  make_fizz_buzz_foo_bar,
  round_trip_fizz_buzz_foo_bar,
} from "./baml_sdk/symbol_collisions/fizz/buzz/foo/index.js";
import { make_ipsum, round_trip_ipsum } from "./baml_sdk/symbol_collisions/lorem/index.js";
import { make_deep, round_trip_deep } from "./baml_sdk/symbol_collisions/a/b/c/d/index.js";

describe("roundtrip symbol_collisions", () => {
  it("symbol_collisions_round_trip_foo_bar", () => {
    const bar = make_foo_bar("hi", 2);
    expect(round_trip_foo_bar(bar)).toEqual(bar);
  });
  it("symbol_collisions_round_trip_fizz_foo_bar", () => {
    const bar = make_fizz_foo_bar("t", 1.5);
    expect(round_trip_fizz_foo_bar(bar)).toEqual(bar);
  });
  it("symbol_collisions_round_trip_fizz_buzz_foo_bar", () => {
    const bar = make_fizz_buzz_foo_bar("f", 2.5, true);
    expect(round_trip_fizz_buzz_foo_bar(bar)).toEqual(bar);
  });
  it("symbol_collisions_round_trip_ipsum", () => {
    const ipsum = make_ipsum(
      make_foo_bar("a", 1),
      make_fizz_foo_bar("b", 2.0),
      make_fizz_buzz_foo_bar("c", 3.0, false),
    );
    expect(round_trip_ipsum(ipsum)).toEqual(ipsum);
  });
  it("symbol_collisions_round_trip_deep", () => {
    const ipsum = make_ipsum(
      make_foo_bar("a", 1),
      make_fizz_foo_bar("b", 2.0),
      make_fizz_buzz_foo_bar("c", 3.0, false),
    );
    const deep = make_deep(
      make_foo_bar("h", 9),
      make_fizz_foo_bar("th", 4.0),
      make_fizz_buzz_foo_bar("fu", 5.0, true),
      ipsum,
    );
    expect(round_trip_deep(deep)).toEqual(deep);
  });
});
