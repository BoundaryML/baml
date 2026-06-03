// TypeScript correspondent to test_main.py: the minimal nullary
// expression function.
//
// `main.baml` declares `hello_world() -> "hello world"`, whose body
// returns the string literal. Calling the generated binding (sync + its
// `_async` sibling) should round-trip that literal back through the engine
// unchanged.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { hello_world, hello_world_async } from "./baml_sdk/index.js";

describe("function_calls — hello_world", () => {
  it("returns the literal (sync)", () => {
    expect(hello_world()).toBe("hello world");
  });

  it("returns the literal (async)", async () => {
    expect(await hello_world_async()).toBe("hello world");
  });
});
