// Smoke tests for plain (non-LLM) expression functions.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { hello_world, single_required_arg } from "./baml_sdk/index.js";

describe("function_calls — hello_world", () => {
  it("test_hello_world_returns_literal", () => {
    expect(hello_world()).toBe("hello world");
  });
});

describe("function_calls — single_required_arg", () => {
  it("test_single_required_arg_round_trips", () => {
    // The next step up from the nullary case: one required positional arg.
    expect(single_required_arg("hi")).toBe("hi");
  });
});
