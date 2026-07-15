// Generic inference coverage ported 1:1 by test name from Python's
// test_generic_inference.py.
//
// Bare calls intentionally omit `$types`: the shared engine infers every
// value-carried TypeVar, just as it does for Python. `$types` is reserved for
// explicit or partial bindings that cannot be expressed by TypeScript's erased
// host generics.
import "./baml_sdk/index.js";
import type { BamlType } from "@boundaryml/baml-bridge";
import { expect, it } from "vitest";
import {
  ContainerShapes,
  GenericBox,
  GenericPair,
  GenericRecursive,
  GenericTriple,
  NamedStatic,
  SomeEnum,
  StringIntPair,
  apply,
  apply_async,
  choose,
  combine,
  elem_type,
  extract,
  first_or,
  glue,
  identity,
  identity_async,
  list_head,
  make_triple,
  maybe_id,
  merge,
  one_type_arg,
  pair,
  parse_as,
  read_items,
  second_of,
  tag_or_value,
  triple_choose,
  two_in_union,
  values_of,
  wrap,
} from "./baml_sdk/generic_tests/index.js";

const parity = it;
const nodeNA = it;

type TypeBindings = Record<string, BamlType>;

function callBare<R>(fn: unknown, args: readonly unknown[]): R {
  return (fn as (...runtimeArgs: unknown[]) => R)(...args);
}

function callWithTypes<R>(
  fn: unknown,
  args: readonly unknown[],
  $types: TypeBindings,
): R {
  return (fn as (...runtimeArgs: unknown[]) => R)(...args, { $types });
}

function expectInferenceError(call: () => unknown, functionName: string): void {
  expect(call).toThrow(/could not infer a type/i);
  expect(call).toThrow(functionName);
}

function boxType(arg: BamlType): BamlType {
  return { class: GenericBox, args: [arg] };
}

function pairType(first: BamlType, second: BamlType): BamlType {
  return { class: GenericPair, args: [first, second] };
}

function boundNestedPair(): GenericPair<
  GenericPair<number, string>,
  GenericPair<boolean, number>
> {
  const leftType = pairType("int", "string");
  const rightType = pairType("bool", "float");
  return new GenericPair({
    first: new GenericPair({
      first: 1,
      second: "a",
      $types: { A: "int", B: "string" },
    }),
    second: new GenericPair({
      first: true,
      second: 1.5,
      $types: { A: "bool", B: "float" },
    }),
    $types: { A: leftType, B: rightType },
  });
}

parity("test_identity_infers_primitives", () => {
  expect(identity(5)).toBe(5);
  expect(identity("hi")).toBe("hi");
  expect(identity(true)).toBe(true);
});

parity("test_identity_infers_user_class", () => {
  const value = new StringIntPair({ my_string: "a", my_int: 1 });
  const result = identity(value);
  expect(result).toBeInstanceOf(StringIntPair);
  expect(result).toMatchObject({ my_string: "a", my_int: 1 });
});

parity("test_identity_infers_generic_instance", () => {
  const box = new GenericBox<number>({ value: 5, $types: { T: "int" } });
  const result = identity(box);
  expect(result).toBeInstanceOf(GenericBox);
  expect(result.$types).toEqual({ T: "int" });
  expect(result.value).toBe(5);

  const inner = new GenericBox<string>({
    value: "hello",
    $types: { T: "string" },
  });
  const nested = new GenericBox<GenericBox<string>>({
    value: inner,
    $types: { T: boxType("string") },
  });
  const nestedResult = identity(nested);
  expect(nestedResult).toBeInstanceOf(GenericBox);
  expect(nestedResult.$types).toEqual({ T: boxType("string") });
  expect(nestedResult.value).toBeInstanceOf(GenericBox);
  expect(nestedResult.value.value).toBe("hello");
});

parity("test_identity_async_infers", async () => {
  await expect(identity_async(7)).resolves.toBe(7);
});

parity("test_identity_null_round_trips", () => {
  expect(identity(null)).toBeNull();
});

parity("test_identity_unbound_generic_instance_round_trips", () => {
  const unbound = new GenericBox<number>({ value: 5 });
  const result = identity(unbound);
  expect(result).toBeInstanceOf(GenericBox);
  expect(result.value).toBe(5);
  expect(result.$types).toBeUndefined();
});

parity("test_make_triple_infers_multiple_typevars", () => {
  const result = make_triple(1, ["a", "b"], { k: true });
  expect(result).toBeInstanceOf(GenericTriple);
  expect(result.first).toBe(1);
  expect(result.second).toEqual(["a", "b"]);
  expect(result.third).toEqual({ k: true });
});

parity("test_second_of_infers_from_nested_generic", () => {
  const strings = new GenericPair<number, string>({
    first: 1,
    second: "hi",
    $types: { A: "int", B: "string" },
  });
  expect(second_of(strings)).toBe("hi");

  const value = new StringIntPair({ my_string: "z", my_int: 9 });
  const classes = new GenericPair<number, StringIntPair>({
    first: 0,
    second: value,
    $types: { A: "int", B: StringIntPair },
  });
  const result = second_of(classes);
  expect(result).toBeInstanceOf(StringIntPair);
  expect(result).toMatchObject({ my_string: "z", my_int: 9 });
});

parity("test_read_items_infers_from_instance_wire_args", () => {
  const populated = new ContainerShapes<number>({
    item: 1,
    items: [1, 2, 3],
    by_key: { k: 4 },
    maybe: null,
    mixed: null,
    $types: { T: "int" },
  });
  expect(read_items(populated)).toEqual([1, 2, 3]);

  const empty = new ContainerShapes<number>({
    item: 1,
    items: [],
    by_key: {},
    maybe: null,
    mixed: null,
    $types: { T: "int" },
  });
  expect(read_items(empty)).toEqual([]);
});

parity("test_list_head_infers_from_recursive_generic", () => {
  const linked = new GenericRecursive<number>({
    value: 7,
    next: new GenericRecursive({
      value: 8,
      next: null,
      $types: { T: "int" },
    }),
    $types: { T: "int" },
  });
  expect(list_head(linked)).toBe(7);
});

parity("test_extract_infers_four_typevars_from_nesting", () => {
  const nested = boundNestedPair();
  expect(extract(nested)).toBe("int | string | bool | float");
});

parity("test_choose_infers_unified_typevar", () => {
  expect(choose(5, 6)).toBe(5);
  expect(choose("a", "b")).toBe("a");
});

parity("test_choose_infers_divergent_union", () => {
  expect(callBare<number | string>(choose, [5, "asdf"])).toBe(5);
});

parity("test_make_triple_partial_explicit_then_infer", () => {
  const result = callWithTypes<GenericTriple<number, string, boolean>>(
    make_triple,
    [1, ["x", "y"], { k: true }],
    { A: "int" },
  );
  expect(result).toBeInstanceOf(GenericTriple);
  expect(result.second).toEqual(["x", "y"]);
});

parity("test_wrap_infers_and_returns_generic", () => {
  const result = wrap(5);
  expect(result).toBeInstanceOf(GenericBox);
  expect(result.$types).toEqual({ T: "int" });
  expect(result.value).toBe(5);
});

parity("test_genericbox_pair_with_infers_method_typevar", () => {
  const box = new GenericBox<number>({ value: 5, $types: { T: "int" } });
  expect(box.pair_with("hello world")).toBe("int | string");
});

parity("test_generic_static_infers_own_typevar", () => {
  const result = GenericBox.new(5);
  expect(result).toBeInstanceOf(GenericBox);
  expect(result.value).toBe(5);
});

parity("test_named_static_infers_distinct_typevars", () => {
  expect(NamedStatic.make(1, "x")).toBe("int | string");
});

parity("test_union_with_concrete_sibling_infers_typevar", () => {
  expect(tag_or_value(5)).toBe("int");
});

parity("test_union_concrete_sibling_absorbs_value_binds_rust_type", () => {
  expect(tag_or_value("hi")).toBe("$rust_type");
});

parity("test_union_null_actual_binds_rust_type", () => {
  expect(tag_or_value(null)).toBe("$rust_type");
});

parity("test_return_only_var_still_requires_binding", () => {
  expectInferenceError(() => parse_as("42"), "parse_as");
});

parity("test_body_only_var_still_requires_binding", () => {
  expectInferenceError(() => one_type_arg(), "one_type_arg");
});

parity("test_pair_invariant_list_conflict_rejects", () => {
  expect(() => callBare(pair, [[1, 2], ["a", "b"]])).toThrow(
    /can't be reconciled/i,
  );
});

parity("test_pair_invariant_list_agree_binds", () => {
  expect(pair([1, 2], [3, 4])).toBe("int");
});

parity("test_choose_union_outside_container_is_sound", () => {
  expect(callBare<number[] | string[]>(choose, [[1, 2], ["a"]])).toEqual([
    1, 2,
  ]);
});

parity("test_merge_invariant_map_value_conflict_rejects", () => {
  expect(() => callBare(merge, [{ k: 1 }, { k: "a" }])).toThrow(
    /can't be reconciled/i,
  );
});

parity("test_combine_invariant_class_arg_conflict_rejects", () => {
  const ints = new GenericBox<number>({ value: 1, $types: { T: "int" } });
  const strings = new GenericBox<string>({
    value: "x",
    $types: { T: "string" },
  });
  expect(() => callBare(combine, [ints, strings])).toThrow(
    /can't be reconciled/i,
  );
});

parity("test_glue_invariant_vs_covariant_conflict_rejects", () => {
  expect(() => callBare(glue, [1, ["a"]])).toThrow(/can't be reconciled/i);
});

parity("test_glue_invariant_and_covariant_agree_binds", () => {
  expect(glue(1, [2, 3])).toBe("int");
});

parity("test_two_typevar_union_is_uninferrable_rejects", () => {
  expectInferenceError(() => two_in_union("hello"), "two_in_union");
});

parity("test_triple_choose_three_covariant_join", () => {
  expect(callBare<string>(triple_choose, [5, "asdf", true])).toBe(
    "int | string | bool",
  );
});

parity("test_make_triple_heterogeneous_list_element_unions", () => {
  const result = make_triple(1, [1, "x"], { k: true });
  expect(result.second).toEqual([1, "x"]);
});

parity("test_choose_divergent_generic_instances_union", () => {
  const left = new GenericBox<number>({ value: 1, $types: { T: "int" } });
  const right = new GenericBox<string>({
    value: "x",
    $types: { T: "string" },
  });
  const result = callBare<GenericBox<number> | GenericBox<string>>(choose, [
    left,
    right,
  ]);
  expect(result).toBeInstanceOf(GenericBox);
  expect(result.value).toBe(1);
});

parity("test_tag_or_value_binds_generic_instance", () => {
  const box = new GenericBox<string>({
    value: "asdf",
    $types: { T: "string" },
  });
  const rendered = tag_or_value(box);
  expect(rendered).toContain("GenericBox");
  expect(rendered).toContain("string");
});

parity("test_first_or_empty_list_round_trips_none", () => {
  expect(first_or([])).toBeNull();
});

parity("test_first_or_nonempty_infers_element", () => {
  expect(first_or([7, 8, 9])).toBe(7);
});

parity("test_values_of_empty_map_round_trips_empty_list", () => {
  expect(values_of({})).toEqual([]);
});

parity("test_values_of_nonempty_returns_values", () => {
  expect(values_of({ a: 1, b: 2 })).toEqual([1, 2]);
});

nodeNA("test_make_triple_partial_subscript_requires_full_arity", () => {
  // TypeScript has no runtime `fn[T]` subscript surface. Its `$types` object is
  // equivalent to Python's `_types=` escape hatch, where partial bindings are
  // intentionally legal and the remaining variables are inferred.
  const result = callWithTypes<GenericTriple<number, string, number>>(
    make_triple,
    [5, ["hello", "world"], { asdf: 5 }],
    { A: "int" },
  );
  expect(result.second).toEqual(["hello", "world"]);
});

parity("test_make_triple_subscript_fully_bound", () => {
  const result = callWithTypes<GenericTriple<number, string, boolean>>(
    make_triple,
    [5, ["x"], { k: true }],
    { A: "int", B: "string", C: "bool" },
  );
  expect(result).toBeInstanceOf(GenericTriple);
  expect(result.first).toBe(5);
  expect(result.second).toEqual(["x"]);
  expect(result.third).toEqual({ k: true });
});

parity("test_one_type_arg_explicit_types_succeeds", () => {
  expect(callWithTypes<string>(one_type_arg, [], { T: "int" })).toBe("int");
});

parity("test_parse_as_explicit_types_succeeds", () => {
  expect(callWithTypes<number>(parse_as, ["42"], { T: "int" })).toBe(42);
});

parity("test_second_of_unbound_instance_recovers_field_type", () => {
  const unbound = new GenericPair<number, string>({ first: 1, second: "hi" });
  expect(second_of(unbound)).toBe("hi");
});

parity("test_identity_nested_unbound_round_trips", () => {
  const nested = new GenericBox<GenericBox<string>>({
    value: new GenericBox<string>({ value: "hello" }),
  });
  const result = identity(nested);
  expect(result).toBeInstanceOf(GenericBox);
  expect(result.$types).toBeUndefined();
  expect(result.value).toBeInstanceOf(GenericBox);
  expect(result.value.$types).toBeUndefined();
  expect(result.value.value).toBe("hello");
});

parity("test_wrap_infers_and_returns_bound_generic", () => {
  const result = wrap(5);
  expect(result.$types).toEqual({ T: "int" });
  expect(result.value).toBe(5);
});

parity("test_maybe_id_present_value_infers", () => {
  expect(maybe_id(5)).toBe(5);
});

parity("test_maybe_id_null_round_trips", () => {
  expect(maybe_id(null)).toBeNull();
});

nodeNA("test_identity_enum_round_trips", () => {
  // A generated TypeScript enum is a runtime string and BamlType currently has
  // no enum token, so inference correctly sees the host value as `string` but
  // cannot preserve Python's distinct runtime enum identity.
  const result = identity(SomeEnum.VARIANT);
  expect(result).toBe(SomeEnum.VARIANT);
  expect(typeof result).toBe("string");
});

parity("test_host_only_object_not_encodable_from_python", () => {
  class HostThing {
    constructor(readonly n: number) {}
  }
  expect(() => identity(new HostThing(3))).toThrow(
    /Cannot encode unregistered class instance HostThing/,
  );
});

nodeNA("test_apply_closure_poisons_typevars_must_specify", async () => {
  // Synchronous Node calls reject host callbacks before the shared inference
  // gate because blocking the JS thread would deadlock callback dispatch. The
  // async entrypoint reaches the same engine inference rejection as Python.
  expect(() => apply((value: number) => value + 1, 5)).toThrow(
    /host callables.*async/i,
  );
  await expect(
    apply_async((value: number) => value + 1, 5),
  ).rejects.toThrow(/could not infer a type/i);
});

nodeNA("test_apply_closure_typevars_specified_succeeds", async () => {
  const callback = (value: unknown) => Number(value) + 1;
  // Node cannot dispatch a host callback from the synchronous bridge path.
  expect(() =>
    callWithTypes(apply, [callback, 5], { T: "int", R: "int" }),
  ).toThrow(/host callables.*async/i);
  await expect(
    callWithTypes<Promise<number>>(apply_async, [callback, 5], {
      T: "int",
      R: "int",
    }),
  ).resolves.toBe(6);
});

parity("test_genericbox_get_infers_class_var_from_receiver", () => {
  const box = new GenericBox<number>({ value: 5, $types: { T: "int" } });
  expect(box.get()).toBe("int");
});

parity("test_genericbox_pair_with_unbound_receiver_recovers_class_var", () => {
  const unbound = new GenericBox<number>({ value: 5 });
  expect(unbound.pair_with("x")).toBe("int | string");
  expect(() =>
    callWithTypes(unbound.pair_with, ["x"], { U: "string" }),
  ).toThrow(/generic receiver carrying its class type args/);
});

parity("test_make_triple_types_kwarg_contradicted_by_actual_rejects", () => {
  expect(() =>
    callWithTypes(make_triple, ["nope", ["x"], { k: true }], { A: "int" }),
  ).toThrow(/make_triple/);
});

parity("test_make_triple_full_subscript_contradicted_by_actual_rejects", () => {
  // `$types` is Node's explicit surface; TypeScript has no Python subscript.
  expect(() =>
    callWithTypes(
      make_triple,
      ["nope", ["x"], { k: true }],
      { A: "int", B: "string", C: "bool" },
    ),
  ).toThrow();
});

parity("test_elem_type_heterogeneous_array_unifies", () => {
  expect(elem_type([1, "x"])).toBe("int | string");
});

parity("test_elem_type_homogeneous_array_is_single_type", () => {
  expect(elem_type([1, 2, 3])).toBe("int");
});

parity("test_elem_type_three_way_heterogeneous_array_unifies", () => {
  const rendered = elem_type([1, "x", true]);
  expect(rendered).toContain("int");
  expect(rendered).toContain("string");
  expect(rendered).toContain("bool");
});

parity("test_read_items_unbound_container_recovers_T_from_fields", () => {
  const unbound = new ContainerShapes<number>({
    item: 1,
    items: [1, 2, 3],
    by_key: { k: 4 },
    maybe: null,
    mixed: null,
  });
  expect(read_items(unbound)).toEqual([1, 2, 3]);
});

parity("test_list_head_unbound_recursive_recovers_T_from_fields", () => {
  const unbound = new GenericRecursive<number>({
    value: 7,
    next: new GenericRecursive<number>({ value: 8, next: null }),
  });
  expect(list_head(unbound)).toBe(7);
});

parity("test_extract_fully_unbound_nested_pair_recovers_all_vars", () => {
  const unbound = new GenericPair<
    GenericPair<number, string>,
    GenericPair<boolean, number>
  >({
    first: new GenericPair({ first: 1, second: "a" }),
    second: new GenericPair({ first: true, second: 1.5 }),
  });
  expect(extract(unbound)).toBe("int | string | bool | float");
});

parity("test_triple_choose_join_includes_concrete_class", () => {
  const value = new StringIntPair({ my_string: "a", my_int: 1 });
  const rendered = callBare<string>(triple_choose, [5, value, "x"]);
  expect(rendered).toContain("int");
  expect(rendered).toContain("StringIntPair");
  expect(rendered).toContain("string");
});

nodeNA("test_triple_choose_join_includes_enum_variant", () => {
  const value = new StringIntPair({ my_string: "a", my_int: 1 });
  // Generated TypeScript string enums are indistinguishable from strings at
  // runtime, so the inferred join names `string` instead of `SomeEnum`.
  const rendered = callBare<string>(triple_choose, [
    5,
    SomeEnum.VARIANT,
    value,
  ]);
  expect(rendered).toContain("int");
  expect(rendered).toContain("string");
  expect(rendered).toContain("StringIntPair");
  expect(rendered).not.toContain("SomeEnum");
});
