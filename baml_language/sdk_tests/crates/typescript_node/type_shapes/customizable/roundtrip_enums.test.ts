// Roundtrip coverage for baml_sdk/enums — ported from test_enums.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Sentiment,
  Enums,
  pick_sentiment,
  pick_positive,
  round_trip_sentiment,
  round_trip_sentiment_positive,
  round_trip_enums,
} from "./baml_sdk/enums/index.js";

describe("roundtrip enums", () => {
  it("test_pick_sentiment", () => {
    expect(pick_sentiment(true)).toBe(Sentiment.Positive);
    expect(pick_sentiment(false)).toBe(Sentiment.Negative);
  });
  it("test_pick_positive", () =>
    expect(pick_positive()).toBe(Sentiment.Positive));
  it("test_round_trip_sentiment", () =>
    expect(round_trip_sentiment(Sentiment.Negative)).toBe(Sentiment.Negative));
  it("test_round_trip_sentiment_positive", () =>
    expect(round_trip_sentiment_positive(Sentiment.Positive)).toBe(
      Sentiment.Positive,
    ));
  it("test_round_trip_enums", () => {
    const e = new Enums({
      bare_enum: Sentiment.Positive,
      variant_as_type: Sentiment.Positive,
    });
    expect(round_trip_enums(e)).toEqual(e);
  });
});
