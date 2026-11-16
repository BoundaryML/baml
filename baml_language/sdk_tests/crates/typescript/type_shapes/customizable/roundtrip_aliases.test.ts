// Roundtrip coverage for baml_sdk/aliases — ported from test_aliases.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  AliasContainer,
  round_trip_string_list_sync,
  round_trip_rec_list_sync,
  round_trip_alias_container_sync,
} from "./baml_sdk/aliases/index.js";

describe("roundtrip aliases", () => {
  it("aliases_round_trip_string_list", () =>
    expect(round_trip_string_list_sync(["a", "b"])).toEqual(["a", "b"]));
  it("aliases_round_trip_rec_list", () => {
    // RecList = int | RecList[]
    expect(round_trip_rec_list_sync(1)).toBe(1);
    expect(round_trip_rec_list_sync([1, [2, 3]])).toEqual([1, [2, 3]]);
  });
  it("aliases_round_trip_alias_container", () => {
    const c = new AliasContainer({ list_field: ["x"], rec_field: [1, [2]] });
    expect(round_trip_alias_container_sync(c)).toEqual(c);
  });
});
