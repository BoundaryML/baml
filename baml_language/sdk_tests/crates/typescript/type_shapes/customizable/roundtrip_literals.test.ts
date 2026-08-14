// Roundtrip coverage for baml_sdk/literals — ported from test_literals.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Literals,
  return_literal42,
  return_literal_neg_one,
  return_literal_draft,
  return_literal_escaped,
  return_literal_true,
  return_literal_false,
  round_trip_literal42,
  round_trip_literal_draft,
  round_trip_literal_escaped,
  round_trip_literal_true,
  round_trip_literal_false,
  round_trip_literals,
} from "./baml_sdk/literals/index.js";

describe("roundtrip literals", () => {
  it("literals_return_literals", () => {
    expect(return_literal42()).toBe(42);
    expect(return_literal_neg_one()).toBe(-1);
    expect(return_literal_draft()).toBe("draft");
    expect(return_literal_escaped()).toBe('has "quotes"');
    expect(return_literal_true()).toBe(true);
    expect(return_literal_false()).toBe(false);
  });
  it("literals_round_trip_literal42", () => expect(round_trip_literal42(42)).toBe(42));
  it("literals_round_trip_literal_draft", () =>
    expect(round_trip_literal_draft("draft")).toBe("draft"));
  it("literals_round_trip_literal_escaped", () =>
    expect(round_trip_literal_escaped('has "quotes"')).toBe('has "quotes"'));
  it("literals_round_trip_literal_true", () => expect(round_trip_literal_true(true)).toBe(true));
  it("literals_round_trip_literal_false", () =>
    expect(round_trip_literal_false(false)).toBe(false));
  it("literals_round_trip_literals", () => {
    const lit = new Literals({
      literal_42: 42,
      literal_draft: "draft",
      literal_escaped: 'has "quotes"',
      literal_true: true,
      literal_false: false,
    });
    expect(round_trip_literals(lit)).toEqual(lit);
  });
});
