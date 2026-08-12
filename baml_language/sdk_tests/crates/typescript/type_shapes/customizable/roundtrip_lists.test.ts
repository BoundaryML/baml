// Roundtrip coverage for baml_sdk/lists — ported from test_lists.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  ListContainer,
  round_trip_ints,
  round_trip_optional_strings,
  round_trip_union_list,
  round_trip_list_container,
} from "./baml_sdk/lists/index.js";

describe("roundtrip lists", () => {
  it("lists_round_trip_ints", () => expect(round_trip_ints([1, 2, 3])).toEqual([1, 2, 3]));
  it("lists_round_trip_empty_list", () => expect(round_trip_ints([])).toEqual([]));
  it("lists_round_trip_optional_strings", () =>
    expect(round_trip_optional_strings(["a", null, "b"])).toEqual(["a", null, "b"]));
  it("lists_round_trip_union_list", () =>
    expect(round_trip_union_list([1, "two", 3])).toEqual([1, "two", 3]));
  it("lists_round_trip_list_container", () => {
    const c = new ListContainer({
      ints: [1, 2],
      optional_strings: [null, "z"],
      union_list: [1, "x"],
    });
    expect(round_trip_list_container(c)).toEqual(c);
  });
});
