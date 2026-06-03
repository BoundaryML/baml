// Known-bug pin for generic instance methods crossing the host boundary.
import "./baml_sdk";
import { describe, it, expect } from "@jest/globals";
import { make_wrapper_methods } from "./baml_sdk/generics";

describe("generic method boundary", () => {
  it.skip("test_generic", () => {
    const w = make_wrapper_methods("hello");
    expect(w.get_value_or_marker()).toBe("hello");
  });
});
