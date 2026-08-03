// Roundtrip coverage for baml_sdk/forward_refs — ported from test_forward_refs.py.
// `round_trip_node` is uninhabitable (required self-ref); import-only.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Other,
  GNode,
  round_trip_other,
  round_trip_rec_list,
  round_trip_rec_list_with_other,
  round_trip_node, // eslint-disable-line @typescript-eslint/no-unused-vars
  round_trip_g_node_int,
} from "./baml_sdk/forward_refs/index.js";

void round_trip_node; // import-only: required self-ref is uninhabitable host-side

describe("roundtrip forward_refs", () => {
  it("forward_refs_round_trip_other", () => {
    const o = new Other({ v: 7 });
    expect(round_trip_other(o)).toEqual(o);
  });
  it("forward_refs_round_trip_rec_list", () =>
    expect(round_trip_rec_list([1, [2, 3]])).toEqual([1, [2, 3]]));
  it("forward_refs_round_trip_rec_list_with_other", () => {
    // RecListWithOther = int | Other | RecListWithOther[]
    expect(round_trip_rec_list_with_other(1)).toBe(1);
    expect(round_trip_rec_list_with_other([1, 2])).toEqual([1, 2]);
  });
  // `GNode<int>` is a generic class passed directly as an argument. The TS
  // encoder now sends it as a FQN-tagged `class_value` with the value-level
  // class type-args channel (`class_ty`), so the engine decodes it to an
  // `Instance` and accepts it into the generic class slot. (No `$types` is
  // passed, so the arg lowers to the unknown/top wildcard.)
  it("forward_refs_round_trip_g_node_int", () => {
    // Leaf node carries children=[] — exercises the empty-list round trip.
    const g = new GNode<number>({ children: [new GNode<number>({ children: [] })] });
    expect(round_trip_g_node_int(g)).toEqual(g);
  });
});
