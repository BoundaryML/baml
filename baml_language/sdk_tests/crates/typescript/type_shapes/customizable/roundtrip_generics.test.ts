// Roundtrip coverage for baml_sdk/generics — ported from test_generics.py.
// (Concretely-instantiated generic class round trips; TS generics are erased.)
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Wrapper,
  GenericLinkedList,
  GenericBinaryTree,
  Box,
  NestedGenerics,
  DifferingInstantiation,
  round_trip_wrapper_int,
  round_trip_generic_linked_list_int,
  round_trip_generic_binary_tree_int,
  round_trip_box_int,
  round_trip_nested_generics,
  round_trip_differing_instantiation,
} from "./baml_sdk/generics/index.js";

describe("roundtrip generics", () => {
  // The four tests below pass a *generic* class instance directly as a function
  // argument. The TS encoder now emits a generic instance as a FQN-tagged
  // `class_value` carrying the value-level class type-args channel (`class_ty`),
  // built from each generic class's static `$generic` param list and optional
  // `$types` field — so the engine decodes it to an `Instance` instead of a
  // bare map and accepts it into the generic class slot. (No `$types` is passed
  // here, so each arg lowers to the unknown/top type, which is a compatible
  // wildcard against the declared concrete args.)
  it("generics_round_trip_wrapper_int", () => {
    const w = new Wrapper({ value: 5 });
    expect(round_trip_wrapper_int(w)).toEqual(w);
  });
  it("generics_round_trip_generic_linked_list_int", () => {
    const ll = new GenericLinkedList({ value: 1, next: new GenericLinkedList({ value: 2, next: null }) });
    expect(round_trip_generic_linked_list_int(ll)).toEqual(ll);
  });
  it("generics_round_trip_generic_binary_tree_int", () => {
    const t = new GenericBinaryTree({ value: 1, left: null, right: null });
    expect(round_trip_generic_binary_tree_int(t)).toEqual(t);
  });
  it("generics_round_trip_box_int", () => {
    const b = new Box({ value: 3, wrapped: new Wrapper({ value: 4 }) });
    expect(round_trip_box_int(b)).toEqual(b);
  });
  it("generics_round_trip_nested_generics", () => {
    const n = new NestedGenerics({
      ww: new Wrapper({ value: new Wrapper({ value: 1 }) }),
      wl: new Wrapper({ value: [1, 2] }),
      wr: new Wrapper({ value: new GenericLinkedList({ value: 9, next: null }) }),
    });
    expect(round_trip_nested_generics(n)).toEqual(n);
  });
  it("generics_round_trip_differing_instantiation", () => {
    const d = new DifferingInstantiation({
      list: new GenericLinkedList({ value: new Wrapper({ value: 1 }), next: null }),
    });
    expect(round_trip_differing_instantiation(d)).toEqual(d);
  });
});
