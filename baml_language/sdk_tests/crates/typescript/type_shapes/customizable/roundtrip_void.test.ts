// Roundtrip coverage for baml_sdk/void — ported from test_void.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { no_op } from "./baml_sdk/void/index.js";

describe("roundtrip void", () => {
  it("void_no_op", () => expect(no_op()).toBeNull());
});
