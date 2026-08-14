// Roundtrip coverage for baml_sdk/recursion — ported from test_recursion.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  IntBinaryTree,
  A,
  B,
  T1,
  T2,
  T3,
  T4,
  T5,
  T6,
  round_trip_int_binary_tree,
  round_trip_a,
  round_trip_b,
  round_trip_t1,
  round_trip_t2,
  round_trip_t3,
  round_trip_t4,
  round_trip_t5,
  round_trip_t6,
} from "./baml_sdk/recursion/index.js";

describe("roundtrip recursion", () => {
  it("recursion_round_trip_int_binary_tree", () => {
    const t = new IntBinaryTree({
      value: 1,
      left: new IntBinaryTree({ value: 2, left: null, right: null }),
      right: null,
    });
    expect(round_trip_int_binary_tree(t)).toEqual(t);
  });
  it("recursion_round_trip_mutual_recursion", () => {
    const a = new A({ b: new B({ a: null }) });
    const b = new B({ a: new A({ b: null }) });
    expect(round_trip_a(a)).toEqual(a);
    expect(round_trip_b(b)).toEqual(b);
  });
  it("recursion_round_trip_scc_t1_t2_t3", () => {
    const t1 = new T1({ via2: new T2({ via1: null, via3: null }), via3: null });
    const t2 = new T2({ via1: null, via3: new T3({ via1: null, via2: null }) });
    const t3 = new T3({ via1: null, via2: null });
    expect(round_trip_t1(t1)).toEqual(t1);
    expect(round_trip_t2(t2)).toEqual(t2);
    expect(round_trip_t3(t3)).toEqual(t3);
  });
  it("recursion_round_trip_scc_t4_t5_t6", () => {
    const t4 = new T4({ via5: new T5({ via4: null, via6: null }), via6: null });
    const t5 = new T5({ via4: null, via6: new T6({ via4: null, via5: null }) });
    const t6 = new T6({ via4: null, via5: null });
    expect(round_trip_t4(t4)).toEqual(t4);
    expect(round_trip_t5(t5)).toEqual(t5);
    expect(round_trip_t6(t6)).toEqual(t6);
  });
});
