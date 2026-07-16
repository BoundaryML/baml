// Static + instance method codegen coverage (ns_methods_on_classes.Greeter).
//
// `raises_test.DocLoader` covers the *shape* of method bindings but its bodies
// always throw, so they are never invoked. `Greeter` has non-throwing bodies,
// so this exercises the full host→engine round-trip for both flavors:
//   - static  → `Greeter.create(name)`        (no `self`, on the class)
//   - instance → `g.who()` / `g.greet(arg)`   (`self` bound via `.bind(this)`)
// each with its `_async` sibling.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { Greeter } from "./baml_sdk/methods_on_classes/index.js";

describe("function_calls — static + instance method bindings", () => {
  it("test_method_bindings_exist", () => {
    // Static bindings hang off the class.
    expect(typeof Greeter.create).toBe("function");
    expect(typeof Greeter.create_async).toBe("function");
    // Instance bindings are `.bind(this)` class fields, present on instances.
    const g = new Greeter({ name: "x" });
    expect(typeof g.who).toBe("function");
    expect(typeof g.who_async).toBe("function");
    expect(typeof g.greet).toBe("function");
    expect(typeof g.greet_async).toBe("function");
  });
});

describe("function_calls — static method round-trip", () => {
  it("test_static_create_round_trips", () => {
    const g = Greeter.create("grace");
    expect(g).toBeInstanceOf(Greeter);
    expect(g.name).toBe("grace");
  });

  it("test_static_create_async_round_trips", async () => {
    const g = await Greeter.create_async("ada");
    expect(g).toBeInstanceOf(Greeter);
    expect(g.name).toBe("ada");
  });
});

describe("function_calls — instance method round-trip", () => {
  it("test_instance_who_round_trips", () => {
    const g = Greeter.create("hopper");
    expect(g.who()).toBe("hopper");
  });

  it("test_instance_who_async_round_trips", async () => {
    const g = await Greeter.create_async("hopper");
    await expect(g.who_async()).resolves.toBe("hopper");
  });

  it("test_instance_greet_with_arg_round_trips", () => {
    const g = Greeter.create("lovelace");
    expect(g.greet("hi")).toBe("hi");
  });

  it("test_instance_greet_async_with_arg_round_trips", async () => {
    const g = await Greeter.create_async("lovelace");
    await expect(g.greet_async("hi")).resolves.toBe("hi");
  });
});
