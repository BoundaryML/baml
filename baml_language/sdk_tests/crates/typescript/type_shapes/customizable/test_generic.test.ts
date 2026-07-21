// Generic instance method crossing the host boundary. `make_wrapper_methods`
// returns a `WrapperMethods<string>`; the outbound decoder repopulates its
// `$types` from the wire `generic_args`, so the subsequent `get_value_or_marker`
// instance-method call recovers `T=string` from the receiver and arrives fully
// bound at the engine's strict generic gate.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { make_wrapper_methods } from "./baml_sdk/generics/index.js";

describe("generic method boundary", () => {
  it("generic_generic", () => {
    const w = make_wrapper_methods("hello");
    expect(w.get_value_or_marker()).toBe("hello");
  });
});
