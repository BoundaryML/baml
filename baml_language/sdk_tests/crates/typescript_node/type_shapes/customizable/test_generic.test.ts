// Known-bug pin for generic instance methods crossing the host boundary.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { make_wrapper_methods } from "./baml_sdk/generics/index.js";

describe("generic method boundary", () => {
  it.skip("test_generic", () => {
    const w = make_wrapper_methods("hello");
    expect(w.get_value_or_marker()).toBe("hello");
  });
});
