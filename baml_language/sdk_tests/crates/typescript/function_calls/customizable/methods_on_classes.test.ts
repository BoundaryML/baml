// Static + instance method codegen coverage (ns_methods_on_classes.Greeter).
//
// `raises_test.DocLoader` covers the *shape* of method bindings but its bodies
// always throw, so they are never invoked. `Greeter` has non-throwing bodies,
// so this exercises the full host→engine round-trip for both flavors:
//   - static  → `Greeter.create(name)`        (no `self`, on the class)
//   - instance → `g.who()` / `g.greet(arg)`   (`self` bound via `.bind(this)`)
// each async, with its blocking `_sync` sibling.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { Greeter } from "./baml_sdk/methods_on_classes/index.js";

describe("function_calls — static + instance method bindings", () => {
  it("methods_on_classes_exposes_sync_plus_async_bindings_for_both_flavors", () => {
    // Static bindings hang off the class.
    expect(typeof Greeter.create_sync).toBe("function");
    expect(typeof Greeter.create).toBe("function");
    // Instance bindings are `.bind(this)` class fields, present on instances.
    const g = new Greeter({ name: "x" });
    expect(typeof g.who_sync).toBe("function");
    expect(typeof g.who).toBe("function");
    expect(typeof g.greet_sync).toBe("function");
    expect(typeof g.greet).toBe("function");
  });
});

describe("function_calls — static method round-trip", () => {
  it("methods_on_classes_create_constructs_a_greeter_async_plus_sync", async () => {
    const g = await Greeter.create("ada");
    expect(g).toBeInstanceOf(Greeter);
    expect(g.name).toBe("ada");

    const g2 = Greeter.create_sync("grace");
    expect(g2).toBeInstanceOf(Greeter);
    expect(g2.name).toBe("grace");
  });
});

describe("function_calls — instance method round-trip", () => {
  it("methods_on_classes_who_returns_a_field_off_self_async_plus_sync", async () => {
    const g = await Greeter.create("hopper");
    expect(await g.who()).toBe("hopper");
    expect(g.who_sync()).toBe("hopper");
  });

  it("methods_on_classes_greet_arg_echoes_a_non_self_argument_async_plus_sync", async () => {
    const g = await Greeter.create("lovelace");
    expect(await g.greet("hi")).toBe("hi");
    expect(g.greet_sync("hi")).toBe("hi");
  });
});
