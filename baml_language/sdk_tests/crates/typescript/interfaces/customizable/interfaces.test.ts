import { expect, test } from "vitest";
import * as baml from "./baml_sdk/index.js";
import { BamlType, Never } from "@boundaryml/baml-bridge";

test("checked_interface_projection_preserves_receiver", async () => {
  const original = await baml.make_unspecified_counter_async(10);
  const integer = BamlType.from("int");
  const bottom = BamlType.from(Never);
  const checked = await original.as_interface(baml.CounterValueRef.type(integer, bottom));
  expect(await checked.update(2)).toBe(12);
  await expect(original.as_interface(baml.CounterValueRef.type(BamlType.from("string"), bottom))).rejects.toThrow();
  await expect(original.as_interface(baml.CounterValueRef.type(integer, BamlType.from("string")))).rejects.toThrow();
  const repeated = await original.as_interface(baml.CounterValueRef.type(integer, bottom));
  original.close();
  checked.close();
  expect(await repeated.update(3)).toBe(15);
  repeated.close();
});

test("checked_interface_projection_survives_reference_release", async () => {
  const original = await baml.make_unspecified_counter_async(10);
  const pending = original.as_interface(baml.CounterValueRef.type(BamlType.from("int"), BamlType.from(Never)));
  original.close();
  const checked = await pending;
  expect(await checked.update(2)).toBe(12);
  checked.close();
});

test("checked_interface_projection_rejects_wrong_bindings", async () => {
  const original = await baml.make_text_decoder_async();
  const text = BamlType.from("string");
  const bottom = BamlType.from(Never);
  await expect(original.as_interface(baml.DecoderRef.type(BamlType.from("int"), bottom))).rejects.toThrow();
  await expect(original.as_interface(baml.DecoderRef.type(text, text))).rejects.toThrow();
  const selected = await original.as_interface(baml.DecoderRef.type(text, bottom));
  selected.close();
  original.close();
});

test("baml_receiver_roundtrip", async () => {
  const greeter = await baml.make_greeter_async("Hello");
  const returned = await baml.pass_greeter_async(greeter);
  expect(await greeter.greet("Ada")).toBe("Hello, Ada!");
  expect(await returned.greet("Grace")).toBe("Hello, Grace!");
  expect(await baml.welcome_async(returned, "Lin")).toBe("Hello, Lin!");
});

test("concrete_implements_interface_input", async () => {
  const greeter = await baml.FriendlyGreeter.new_async("Hello");
  expect(greeter).toBeInstanceOf(baml.FriendlyGreeter);
  expect(await greeter.greet("Grace")).toBe("Hello, Grace!");
  expect(await greeter.label()).toBe("greeter");
  expect(await baml.welcome_async(greeter, "Ada")).toBe("Hello, Ada!");
  greeter.close();
});

test("default_method_dispatch", async () => {
  const greeter = await baml.make_greeter_async("Hello");
  expect(await greeter.label()).toBe("greeter");
  expect(await baml.greeter_label_async(greeter)).toBe("greeter");
});

test("owner_state_survives_method_calls", async () => {
  const counter = await baml.make_counter_async(10);
  const returned = await baml.pass_counter_async(counter);
  expect(await counter.add(2)).toBe(12);
  expect(await baml.add_in_baml_async(returned, 5)).toBe(17);
  expect(await counter.current()).toBe(17);
  expect(await returned.current()).toBe(17);
});

test("record_copy_preserves_live_child", async () => {
  const counter = await baml.make_counter_async(10);
  const record = await baml.counter_record_async(counter);
  record.title = "local edit";
  expect(await record.counter.add(2)).toBe(12);
  expect(await counter.current()).toBe(12);
  const fresh = await baml.counter_record_async(counter);
  expect(fresh.title).toBe("counter");
  expect(await fresh.counter.current()).toBe(12);
});


test("copied_record_implements_interface_input", async () => {
  const record = await baml.marker_record_async("initial");
  const retained = await baml.pass_marker_async(record);
  record.text = "local edit";
  expect(await baml.marker_text_async(retained)).toBe("initial");
  expect(await baml.marker_text_async(record)).toBe("local edit");
});

test("inherited_method_preserves_receiver_state", async () => {
  const counter = await baml.make_extended_counter_async(10);
  expect(await counter.add(2)).toBe(12);
  const parent = await baml.pass_counter_async(counter);
  expect(await parent.add(3)).toBe(15);
  expect(await counter.current()).toBe(15);
});

test("inherited_call_survives_reference_release", async () => {
  const counter = await baml.make_extended_counter_async(10);
  const pending = counter.add(2);
  counter.close();
  expect(await pending).toBe(12);
});

test("callback_argument_preserves_receiver", async () => {
  let retained: baml.CounterRef | undefined;
  expect(await baml.visit_counter_async(10, async counter => {
    expect(counter).toBeInstanceOf(baml.CounterRef);
    retained = counter;
    return await counter.add(2);
  })).toBe(12);
  try {
    expect(await retained!.add(3)).toBe(15);
    expect(await baml.add_in_baml_async(retained!, 5)).toBe(20);
  } finally {
    retained?.close();
  }
});

test("async_callback_alias_preserves_receiver", async () => {
  expect(await baml.visit_counter_alias_async(10, async counter => {
    try { return await counter.add(2); }
    finally { counter.close(); }
  })).toBe(12);
});

test("optional_callback_preserves_keyword_arguments", async () => {
  const seen: number[] = [];
  expect(await baml.visit_counter_optional_async(10, async (counter, options = {}) => {
    const amount = options.amount ?? 1;
    seen.push(amount);
    try { return await counter.add(amount); }
    finally { counter.close(); }
  })).toBe(23);
  expect(seen).toEqual([1, 2]);
});

test("nested_callback_preserves_interface_receiver", async () => {
  expect(await baml.use_counter_factory_callback_async(async factory => {
    const counter = factory();
    try {
      expect(counter).toBeInstanceOf(baml.CounterRef);
      return await counter.add(2);
    } finally {
      counter.close();
    }
  })).toBe(12);
});

test("generic_callback_preserves_optional_type_relationship", async () => {
  expect(await baml.visit_generic_callback_async(3,
    async (value: number, options: { other?: number } = {}) => value + (options.other ?? 0),
    { $types: { T: BamlType.from("int") } },
  )).toBe(6);
});

test("interface_method_callback_preserves_optional_types", async () => {
  const runner = await baml.make_counter_runner_async(10);
  try {
    expect(await runner.visit(async (counter, options = {}) => {
      try { return await counter.add(options.amount ?? 1); }
      finally { counter.close(); }
    })).toBe(12);
    expect(await runner.choose(3,
      async (value, options = {}) => value + (options.other ?? 0),
      { $types: { T: BamlType.from("int") } },
    )).toBe(6);
  } finally {
    runner.close();
  }
});

test("interface_container_inputs_accept_implementations", async () => {
  const counter = await baml.make_stored_counter_async(10);
  expect(counter).toBeInstanceOf(baml.StoredCounter);
  try {
    expect(await baml.add_counter_list_async(Object.freeze([counter]), 2)).toEqual([12]);
    expect(await baml.add_counter_list_async([counter] as const, 3)).toEqual([15]);
    expect(await baml.add_counter_map_async(Object.freeze({ counter }), 4)).toEqual([19]);
    expect(await counter.current()).toBe(19);
  } finally {
    counter.close();
  }
});

test("awaitable_callback_produces_interface_result", async () => {
  const counter = await baml.call_counter_factory_async(
    () => Promise.resolve(baml.make_stored_counter_async(10)),
  );
  try {
    expect(counter).toBeInstanceOf(baml.CounterRef);
    expect(await counter.add(2)).toBe(12);
  } finally {
    counter.close();
  }
});

test("generic_interface_method_preserves_type_arguments", async () => {
  const bounded = await baml.make_bounded_method_async();
  try {
    expect(await bounded.check({ $types: { T: baml.MarkedRecordType(), U: BamlType.from("int") } })).toBe("checked");
  } finally { bounded.close(); }
  const echo = await baml.make_echo_async();
  const counter = await baml.make_counter_async(10);
  const record = await baml.counter_record_async(counter);
  try {
    const text: string = await echo.echo("Ada", { $types: { T: BamlType.from("string") } });
    expect(text).toBe("Ada");
    const flavor: baml.InterfaceFlavor = await echo.echo(baml.InterfaceFlavor.Vanilla, {
      $types: { T: baml.InterfaceFlavorType() },
    });
    expect(flavor).toBe(baml.InterfaceFlavor.Vanilla);
    expect(await echo.echo([flavor], { $types: { T: baml.InterfaceFlavorType().array() } })).toEqual([flavor]);
    expect(await echo.echo(flavor, { $types: { T: baml.InterfaceFlavorType().optional() } })).toBe(flavor);
    expect(await echo.echo(null, { $types: { T: baml.InterfaceFlavorType().optional() } })).toBeNull();
    const copied: baml.CounterRecord = await echo.echo(record, { $types: { T: baml.CounterRecordType() } });
    try {
      expect(copied).toBeInstanceOf(baml.CounterRecord);
      expect(await copied.counter.add(2)).toBe(12);
      expect(await counter.current()).toBe(12);
    } finally { copied.counter.close(); }
  } finally { record.counter.close(); counter.close(); echo.close(); }
});

test("generic_interface_method_rejects_wrong_type_arguments", async () => {
  const bounded = await baml.make_bounded_method_async();
  try {
    await expect(Reflect.apply(bounded.check, bounded, [{ $types: { T: BamlType.from("int"), U: BamlType.from("int") } }])).rejects.toThrow();
  } finally { bounded.close(); }
  const echo = await baml.make_echo_async();
  try {
    // Reflect.apply exercises JavaScript callers that have no static checker.
    await expect(Reflect.apply(echo.echo, echo, [42, { $types: { T: BamlType.from("string") } }])).rejects.toThrow();
    await expect(Reflect.apply(echo.echo, echo, ["Missing", { $types: { T: baml.InterfaceFlavorType() } }])).rejects.toThrow();
    expect(await echo.echo("Ada", { $types: { T: BamlType.from("string") } })).toBe("Ada");
  } finally { echo.close(); }
});
