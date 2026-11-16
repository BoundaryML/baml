// Roundtrip coverage for baml_sdk/literals — ported from test_literals.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Literals,
  return_literal42_sync,
  return_literal_neg_one_sync,
  return_literal_draft_sync,
  return_literal_escaped_sync,
  return_literal_true_sync,
  return_literal_false_sync,
  round_trip_literal42_sync,
  round_trip_literal_draft_sync,
  round_trip_literal_escaped_sync,
  round_trip_literal_true_sync,
  round_trip_literal_false_sync,
  round_trip_literals_sync,
} from "./baml_sdk/literals/index.js";

describe("roundtrip literals", () => {
  it("literals_return_literals", () => {
    expect(return_literal42_sync()).toBe(42);
    expect(return_literal_neg_one_sync()).toBe(-1);
    expect(return_literal_draft_sync()).toBe("draft");
    expect(return_literal_escaped_sync()).toBe('has "quotes"');
    expect(return_literal_true_sync()).toBe(true);
    expect(return_literal_false_sync()).toBe(false);
  });
  it("literals_round_trip_literal42", () => expect(round_trip_literal42_sync(42)).toBe(42));
  it("literals_round_trip_literal_draft", () =>
    expect(round_trip_literal_draft_sync("draft")).toBe("draft"));
  it("literals_round_trip_literal_escaped", () =>
    expect(round_trip_literal_escaped_sync('has "quotes"')).toBe('has "quotes"'));
  it("literals_round_trip_literal_true", () => expect(round_trip_literal_true_sync(true)).toBe(true));
  it("literals_round_trip_literal_false", () =>
    expect(round_trip_literal_false_sync(false)).toBe(false));
  it("literals_round_trip_literals", () => {
    const lit = new Literals({
      literal_42: 42,
      literal_draft: "draft",
      literal_escaped: 'has "quotes"',
      literal_true: true,
      literal_false: false,
    });
    expect(round_trip_literals_sync(lit)).toEqual(lit);
  });
});
