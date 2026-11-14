import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { BamlError, BamlType } from "@boundaryml/baml-bridge";
import {
  TupleHolder,
  TuplePairBox,
  TupleBox,
  round_trip_pair_sync,
  round_trip_pair,
  round_trip_single_sync,
  round_trip_repeated_sync,
  round_trip_nested_sync,
  round_trip_optional_slot_sync,
  round_trip_optional_tuple_sync,
  round_trip_tuple_list_sync,
  round_trip_tuple_map_sync,
  round_trip_tuple_or_string_sync,
  round_trip_tuple_arms_sync,
  tuple_or_list_is_tuple_sync,
  round_trip_holder_sync,
  round_trip_tuple_box_sync,
  make_pair_sync,
  make_tuple_swapper_sync,
  swap_pair_sync,
  pair_first_sync,
  call_tuple_callback_sync,
  call_tuple_callback,
  throw_pair_sync,
  round_trip_wide_sync,
  tuple_as_unknown_sync,
  unknown_has_carried_tuple_type_sync,
  value_has_type_sync,
  call_unknown_tuple_callback,
} from "./baml_sdk/tuples/index.js";

describe("roundtrip tuples", () => {
  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_pair", () => {
    expect(round_trip_pair_sync([1, "a"])).toEqual([1, "a"]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_pair_async", async () => {
    expect(await round_trip_pair([2, "b"])).toEqual([2, "b"]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_single_and_repeated", () => {
    expect(round_trip_single_sync(["only"])).toEqual(["only"]);
    expect(round_trip_repeated_sync([1, 2, 3])).toEqual([1, 2, 3]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_nested", () => {
    expect(round_trip_nested_sync([[1, "a"], true])).toEqual([[1, "a"], true]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_optional_slot_and_optional_tuple", () => {
    expect(round_trip_optional_slot_sync([1, null])).toEqual([1, null]);
    expect(round_trip_optional_tuple_sync(null)).toBeNull();
    expect(round_trip_optional_tuple_sync([1, "a"])).toEqual([1, "a"]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_containers", () => {
    expect(round_trip_tuple_list_sync([["a", 1], ["b", 2]])).toEqual([["a", 1], ["b", 2]]);
    expect(round_trip_tuple_map_sync({ k: [1, true] })).toEqual({ k: [1, true] });
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_wide", () => {
    const wide: [number, string, number, string, number, string, number, string, number, string, number, string, number] =
      [1, "a", 2, "b", 3, "c", 4, "d", 5, "e", 6, "f", 7];
    expect(round_trip_wide_sync(wide)).toEqual([...wide]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_union_arms", () => {
    expect(round_trip_tuple_or_string_sync([1, "a"])).toEqual([1, "a"]);
    expect(round_trip_tuple_or_string_sync("s")).toBe("s");
    expect(round_trip_tuple_arms_sync([1, "a"])).toEqual([1, "a"]);
    expect(round_trip_tuple_arms_sync(["a", 1])).toEqual(["a", 1]);
    expect(() => round_trip_tuple_arms_sync([true, 1] as never)).toThrow();
  });

  // SDK_PARITY_LINT(skip): only a TypeScript array is both a list and a tuple
  it("tuples_array_fitting_tuple_and_list_is_tuple", () => {
    expect(tuple_or_list_is_tuple_sync([1, 2])).toBe(true);
    expect(tuple_or_list_is_tuple_sync([1, 2, 3])).toBe(false);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_class_fields", () => {
    const holder = new TupleHolder({
      pair: [1, "a"],
      nested: [[2, "b"], false],
      maybe: [3, null],
      pairs: [["x", 4]],
    });
    expect(round_trip_holder_sync(holder)).toEqual(holder);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_generic_argument", () => {
    const box = new TupleBox<[number, string]>({ value: [1, "a"] });
    const back = round_trip_tuple_box_sync(box);
    expect(back).toEqual(box);
    expect((back as unknown as { $types?: unknown }).$types).toEqual({ T: { tuple: ["int", "string"] } });
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_make_swap_and_index", () => {
    expect(make_pair_sync(1, "a")).toEqual([1, "a"]);
    expect(swap_pair_sync([1, "a"])).toEqual(["a", 1]);
    expect(pair_first_sync([7, "a"])).toBe(7);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_wrong_arity_and_slots_rejected", () => {
    expect(() => round_trip_pair_sync([1] as never)).toThrow();
    expect(() => round_trip_pair_sync([1, "a", 2] as never)).toThrow();
    expect(() => round_trip_pair_sync(["a", 1] as never)).toThrow();
    expect(() => round_trip_optional_slot_sync([1] as never)).toThrow();
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_host_callback_args_and_results", async () => {
    const seen: unknown[] = [];
    const swapped = await call_tuple_callback((t) => {
      seen.push(t);
      return [t[1], t[0]];
    }, [1, "a"]);
    expect(seen).toEqual([[1, "a"]]);
    expect(swapped).toEqual(["a", 1]);
    await expect(call_tuple_callback(() => ["a"] as never, [1, "a"])).rejects.toThrow();
    expect(() => call_tuple_callback_sync((t) => [t[1], t[0]], [1, "a"])).toThrow();
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_thrown_value", () => {
    try {
      throw_pair_sync([1, "a"]);
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(BamlError);
      expect((error as BamlError).value).toEqual([1, "a"]);
    }
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_dynamic_value_keeps_carried_type", async () => {
    const held = tuple_as_unknown_sync([1, true]);
    expect(held).toEqual([1, true]);
    expect(unknown_has_carried_tuple_type_sync(held)).toBe(true);
    expect(unknown_has_carried_tuple_type_sync([1, true])).toBe(false);
    expect(await call_unknown_tuple_callback((v) => v, [1, true])).toBe(true);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_reflected_type_token", () => {
    expect(BamlType.from({ tuple: ["int", "string"] })).toBeInstanceOf(BamlType);
    expect(() => BamlType.from({ tuple: [] })).toThrow();
    const pair = make_pair_sync(1, "a");
    expect(value_has_type_sync(pair, { $types: { T: { tuple: ["int", "string"] } } })).toBe(true);
    expect(value_has_type_sync(pair, { $types: { T: { tuple: ["string", "int"] } } })).toBe(false);
    expect(value_has_type_sync(pair, { $types: { T: { list: { union: ["int", "string"] } } } })).toBe(false);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_method_round_trip", async () => {
    const box = new TuplePairBox({ a: 1, b: "a" });
    expect(box.as_pair_sync()).toEqual([1, "a"]);
    expect(box.with_pair_sync([2, "b"])).toMatchObject({ a: 2, b: "b" });
    expect(TuplePairBox.from_pair_sync([3, "c"])).toBeInstanceOf(TuplePairBox);
    expect(TuplePairBox.from_pair_sync([3, "c"])).toMatchObject({ a: 3, b: "c" });
    expect(await box.as_pair()).toEqual([1, "a"]);
    expect(await box.with_pair([4, "d"])).toMatchObject({ a: 4, b: "d" });
    expect(await TuplePairBox.from_pair([5, "e"])).toMatchObject({ a: 5, b: "e" });
    expect(() => box.with_pair_sync([1] as never)).toThrow();
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_function_handle_round_trip", async () => {
    const swap = make_tuple_swapper_sync();
    expect(typeof swap).toBe("function");
    expect(swap.callSync([1, "a"])).toEqual(["a", 1]);
    expect(swap.callSync([2, "b"])).toEqual(["b", 2]);
    expect(await swap([3, "c"])).toEqual(["c", 3]);
    expect(() => swap.callSync(["a", 1] as never)).toThrow();
  });
});
