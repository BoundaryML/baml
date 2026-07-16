// Roundtrip coverage for baml_sdk/maps — ported from test_maps.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Sentiment,
  Resume,
  round_trip_simple_map,
  round_trip_list_valued_map,
  round_trip_sentiment,
  round_trip_resume,
} from "./baml_sdk/maps/index.js";

describe("roundtrip maps", () => {
  it("test_round_trip_simple_map", () =>
    expect(round_trip_simple_map({ a: 1, b: 2 })).toEqual({ a: 1, b: 2 }));
  it("test_round_trip_list_valued_map", () =>
    expect(round_trip_list_valued_map({ k: [1, 2] })).toEqual({ k: [1, 2] }));
  it("test_round_trip_sentiment", () =>
    expect(round_trip_sentiment(Sentiment.Positive)).toBe(Sentiment.Positive));
  it("test_round_trip_resume", () => {
    const r = new Resume({ name: "n" });
    expect(round_trip_resume(r)).toEqual(r);
  });
});
