// Roundtrip coverage for baml_sdk/void — ported from test_void.py.
import "./baml_sdk";
import { describe, it, expect } from "@jest/globals";
import { no_op } from "./baml_sdk/void";

describe("roundtrip void", () => {
  it("no_op returns null", () => expect(no_op()).toBeNull());
});
