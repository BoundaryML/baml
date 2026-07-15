// Generic function-call coverage ported 1:1 by test name from Python's
// test_generic_calls.py. TypeScript erases generic arguments at runtime, so
// generic calls carry their BAML type bindings in a trailing `$types` option
// and generic instances carry class bindings in their `$types` metadata.
import "./baml_sdk/index.js";
import type { BamlType } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import {
  ContainerShapes,
  GenericBox,
  GenericPair,
  GenericRecursive,
  GenericTriple,
  NamedStatic,
  StringIntPair,
  choose,
  consume_int_wrapper,
  extract,
  identity,
  identity_async,
  list_head,
  make_int_box,
  make_int_container,
  make_int_str_bool_triple,
  make_nested_box,
  make_triple,
  one_type_arg,
  parse_as,
  read_items,
  second_of,
  tag_or_value,
  two_type_args,
  wrap,
} from "./baml_sdk/generic_tests/index.js";

type TypeBindings = Record<string, BamlType>;

// Generated function types expose the compile-time TypeScript generics, while
// `$types` is the separate runtime binding channel consumed by the Node bridge.
// Keep that escape hatch in one place so every test spells the wire contract.
function callWithTypes<R>(
  fn: unknown,
  args: readonly unknown[],
  $types: TypeBindings,
): R {
  return (fn as (...runtimeArgs: unknown[]) => R)(...args, { $types });
}

const genericBoxOfString: BamlType = {
  class: GenericBox,
  args: ["string"],
};
const genericBoxOfInt: BamlType = { class: GenericBox, args: ["int"] };

describe("generic function calls", () => {
  it("test_identity_explicit", () => {
    expect(callWithTypes<number>(identity, [5], { T: "int" })).toBe(5);
    expect(callWithTypes<string>(identity, ["hi"], { T: "string" })).toBe(
      "hi",
    );

    const pair = new StringIntPair({ my_string: "a", my_int: 1 });
    const pairResult = callWithTypes<StringIntPair>(identity, [pair], {
      T: StringIntPair,
    });
    expect(pairResult).toBeInstanceOf(StringIntPair);
    expect(pairResult).toMatchObject({ my_string: "a", my_int: 1 });

    const inner = new GenericBox<string>({
      value: "hello",
      $types: { T: "string" },
    });
    const box = new GenericBox<GenericBox<string>>({
      value: inner,
      $types: { T: genericBoxOfString },
    });
    const boxResult = callWithTypes<GenericBox<GenericBox<string>>>(
      identity,
      [box],
      { T: { class: GenericBox, args: [genericBoxOfString] } },
    );
    expect(boxResult).toBeInstanceOf(GenericBox);
    // The outer box carries `GenericBox<string>` as its own T binding; the
    // explicit function-level T binding is one layer higher and is not copied
    // into the returned value's instance metadata.
    expect(boxResult.$types).toEqual({
      T: genericBoxOfString,
    });
    expect(boxResult.value).toBeInstanceOf(GenericBox);
    expect(boxResult.value.$types).toEqual({ T: "string" });
    expect(boxResult.value.value).toBe("hello");

    const triple = new GenericTriple<GenericBox<string>, number, boolean>({
      first: inner,
      second: [1.1, 2.2],
      third: { lorem: true, ipsum: false },
      $types: { A: genericBoxOfString, B: "float", C: "bool" },
    });
    const tripleResult = callWithTypes<
      GenericTriple<GenericBox<string>, number, boolean>
    >(identity, [triple], {
      T: {
        class: GenericTriple,
        args: [genericBoxOfString, "float", "bool"],
      },
    });
    expect(tripleResult).toBeInstanceOf(GenericTriple);
    expect(tripleResult.first).toBeInstanceOf(GenericBox);
    expect(tripleResult.first.value).toBe("hello");
    expect(tripleResult.second).toEqual([1.1, 2.2]);
    expect(tripleResult.third).toEqual({ lorem: true, ipsum: false });
  });

  it("test_identity_async_explicit", async () => {
    await expect(
      callWithTypes<Promise<number>>(identity_async, [7], { T: "int" }),
    ).resolves.toBe(7);
  });

  it("test_tag_or_value_explicit", () => {
    expect(callWithTypes<string>(tag_or_value, [5], { T: "int" })).toBe(
      "int",
    );
    expect(
      callWithTypes<string>(tag_or_value, ["plain"], { T: "string" }),
    ).toBe("string");
    const pair = new StringIntPair({ my_string: "b", my_int: 2 });
    expect(
      callWithTypes<string>(tag_or_value, [pair], { T: StringIntPair }),
    ).toContain("StringIntPair");
  });

  it("test_make_triple_explicit", () => {
    const triple = callWithTypes<GenericTriple<number, string, boolean>>(
      make_triple,
      [1, ["a", "b"], { k: true }],
      { A: "int", B: "string", C: "bool" },
    );
    expect(triple).toBeInstanceOf(GenericTriple);
    expect(triple.$types).toEqual({ A: "int", B: "string", C: "bool" });
    expect(triple.first).toBe(1);
    expect(triple.second).toEqual(["a", "b"]);
    expect(triple.third).toEqual({ k: true });
  });

  it("test_one_type_arg_explicit", () => {
    expect(callWithTypes<string>(one_type_arg, [], { T: "int" })).toBe(
      "int",
    );
    expect(callWithTypes<string>(one_type_arg, [], { T: "string" })).toBe(
      "string",
    );
    const nested = callWithTypes<string>(one_type_arg, [], {
      T: genericBoxOfInt,
    });
    expect(nested).toContain("GenericBox");
    expect(nested).toContain("int");
  });

  it("test_two_type_args_explicit", () => {
    expect(
      callWithTypes<string>(two_type_args, [], { A: "int", B: "string" }),
    ).toBe("int | string");
  });

  it("test_generic_free_fn_requires_binding", () => {
    // Bare calls reach the shared inference gate. These variables occur only
    // in the body/return, so no argument can provide evidence for them.
    expect(() => one_type_arg()).toThrow(/could not infer a type/i);
    expect(() => two_type_args()).toThrow(/could not infer a type/i);
  });

  it("test_subscript_wrong_arity_raises", () => {
    // TypeScript has no runtime `fn[T]` subscript operation. `$types` matches
    // Python's partial `_types=` form instead: A is seeded and the uninferable
    // return-only B is rejected by the shared engine gate.
    expect(() =>
      callWithTypes<string>(two_type_args, [], { A: "int" }),
    ).toThrow(/could not infer a type/i);
  });

  it("test_consume_int_wrapper_baseline", () => {
    const box = new GenericBox<number>({
      value: 9,
      $types: { T: "int" },
    });
    expect(consume_int_wrapper(box)).toBe(9);
  });

  it("test_genericbox_get_explicit", () => {
    const box = new GenericBox<number>({
      value: 5,
      $types: { T: "int" },
    });
    expect(box.get()).toBe("int");
  });

  it("test_genericbox_pair_with_explicit", () => {
    const box = new GenericBox<number>({
      value: 5,
      $types: { T: "int" },
    });
    expect(
      callWithTypes<string>(box.pair_with, ["hello world"], {
        U: "string",
      }),
    ).toBe("int | string");
  });

  it("test_genericbox_new_static_explicit", () => {
    const box = callWithTypes<GenericBox<number>>(GenericBox.new, [5], {
      V: "int",
    });
    expect(box).toBeInstanceOf(GenericBox);
    expect(box.$types).toEqual({ T: "int" });
    expect(box.value).toBe(5);
  });

  it("test_generic_static_infers_binding", () => {
    const box = GenericBox.new(5);
    expect(box).toBeInstanceOf(GenericBox);
    expect(box.value).toBe(5);
  });

  it("test_named_static_distinct_typevar_names", () => {
    expect(
      callWithTypes<string>(NamedStatic.make, [1, "x"], {
        D: "int",
        E: "string",
      }),
    ).toBe("int | string");
  });

  it("test_instance_method_unparameterized_receiver_raises", () => {
    const box = new GenericBox<number>({ value: 5 });
    expect(() =>
      callWithTypes<string>(box.pair_with, ["x"], { U: "string" }),
    ).toThrow(
      "$types on a generic method requires a generic receiver carrying its class type args",
    );
  });

  it("test_extract_explicit", () => {
    const leftType: BamlType = {
      class: GenericPair,
      args: ["int", "string"],
    };
    const rightType: BamlType = {
      class: GenericPair,
      args: ["bool", "float"],
    };
    const pair = new GenericPair<
      GenericPair<number, string>,
      GenericPair<boolean, number>
    >({
      first: new GenericPair<number, string>({
        first: 1,
        second: "a",
        $types: { A: "int", B: "string" },
      }),
      second: new GenericPair<boolean, number>({
        first: true,
        second: 1.5,
        $types: { A: "bool", B: "float" },
      }),
      $types: { A: leftType, B: rightType },
    });
    expect(
      callWithTypes<string>(extract, [pair], {
        A: "int",
        B: "string",
        C: "bool",
        D: "float",
      }),
    ).toBe("int | string | bool | float");
  });

  it("test_parse_as_explicit", () => {
    const pair = callWithTypes<StringIntPair>(
      parse_as,
      ['{"my_string": "x", "my_int": 3}'],
      { T: StringIntPair },
    );
    expect(pair).toBeInstanceOf(StringIntPair);
    expect(pair).toMatchObject({ my_string: "x", my_int: 3 });
    expect(callWithTypes<number>(parse_as, ["42"], { T: "int" })).toBe(42);
  });

  it("test_second_of_explicit", () => {
    const stringPair = new GenericPair<number, string>({
      first: 1,
      second: "hi",
      $types: { A: "int", B: "string" },
    });
    expect(
      callWithTypes<string>(second_of, [stringPair], { T: "string" }),
    ).toBe("hi");

    const value = new StringIntPair({ my_string: "z", my_int: 9 });
    const classPair = new GenericPair<number, StringIntPair>({
      first: 0,
      second: value,
      $types: { A: "int", B: StringIntPair },
    });
    const result = callWithTypes<StringIntPair>(second_of, [classPair], {
      T: StringIntPair,
    });
    expect(result).toBeInstanceOf(StringIntPair);
    expect(result).toMatchObject({ my_string: "z", my_int: 9 });
  });

  it("test_list_head_explicit", () => {
    const tail = new GenericRecursive<number>({
      value: 8,
      next: null,
      $types: { T: "int" },
    });
    const linkedList = new GenericRecursive<number>({
      value: 7,
      next: tail,
      $types: { T: "int" },
    });
    expect(
      callWithTypes<number>(list_head, [linkedList], { T: "int" }),
    ).toBe(7);
  });

  it("test_choose_explicit", () => {
    expect(callWithTypes<number>(choose, [1, 2], { T: "int" })).toBe(1);
    expect(callWithTypes<string>(choose, ["a", "b"], { T: "string" })).toBe(
      "a",
    );
  });

  it("test_read_items_explicit", () => {
    const container = new ContainerShapes<number>({
      item: 1,
      items: [1, 2, 3],
      by_key: { k: 4 },
      maybe: null,
      mixed: null,
      $types: { T: "int" },
    });
    expect(
      callWithTypes<number[]>(read_items, [container], { T: "int" }),
    ).toEqual([1, 2, 3]);
  });

  it("test_wrap_explicit", () => {
    const wrapped = callWithTypes<GenericBox<number>>(wrap, [5], {
      T: "int",
    });
    expect(wrapped).toBeInstanceOf(GenericBox);
    expect(wrapped.$types).toEqual({ T: "int" });
    expect(wrapped.value).toBe(5);
  });

  it("test_make_int_box_reified", () => {
    const box = make_int_box();
    expect(box).toBeInstanceOf(GenericBox);
    expect(box.$types).toEqual({ T: "int" });
    expect(box.value).toBe(7);
  });

  it("test_make_int_container_reified", () => {
    const container = make_int_container();
    expect(container).toBeInstanceOf(ContainerShapes);
    expect(container.$types).toEqual({ T: "int" });
    expect(container.item).toBe(1);
    expect(container.items).toEqual([1, 2, 3]);
    expect(container.by_key).toEqual({ k: 4 });
    expect(container.maybe).toBeNull();
    expect(container.mixed).toBe(5);
  });

  it("test_make_nested_box_reified", () => {
    const outer = make_nested_box();
    expect(outer).toBeInstanceOf(GenericBox);
    expect(outer.$types).toEqual({ T: genericBoxOfInt });
    expect(outer.value).toBeInstanceOf(GenericBox);
    expect(outer.value.$types).toEqual({ T: "int" });
    expect(outer.value.value).toBe(9);
  });

  it("test_make_int_str_bool_triple_reified", () => {
    const triple = make_int_str_bool_triple();
    expect(triple).toBeInstanceOf(GenericTriple);
    expect(triple.$types).toEqual({ A: "int", B: "string", C: "bool" });
    expect(triple.first).toBe(1);
    expect(triple.second).toEqual(["a", "b"]);
    expect(triple.third).toEqual({ k: true });
  });
});
