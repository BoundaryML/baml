// Smoke tests for plain (non-LLM) expression functions.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  hello_world,
  hello_world_async,
  single_required_arg,
} from "./baml_sdk/index.js";

describe("function_calls — hello_world", () => {
  it("returns the literal (sync)", () => {
    expect(hello_world()).toBe("hello world");
  });

  it("returns the literal (async)", async () => {
    expect(await hello_world_async()).toBe("hello world");
  });
});

describe("function_calls — single_required_arg", () => {
  it("round-trips a single positional argument", () => {
    // The next step up from the nullary case: one required positional arg.
    expect(single_required_arg("hi")).toBe("hi");
  });
});
