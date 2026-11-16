// Roundtrip coverage for baml_sdk/enums — ported from test_enums.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Sentiment,
  Enums,
  pick_sentiment_sync,
  pick_positive_sync,
  round_trip_sentiment_sync,
  round_trip_sentiment_positive_sync,
  round_trip_enums_sync,
} from "./baml_sdk/enums/index.js";

describe("roundtrip enums", () => {
  it("enums_pick_sentiment", () => {
    expect(pick_sentiment_sync(true)).toBe(Sentiment.Positive);
    expect(pick_sentiment_sync(false)).toBe(Sentiment.Negative);
  });
  it("enums_pick_positive", () => expect(pick_positive_sync()).toBe(Sentiment.Positive));
  it("enums_round_trip_sentiment", () =>
    expect(round_trip_sentiment_sync(Sentiment.Negative)).toBe(Sentiment.Negative));
  it("enums_round_trip_sentiment_positive", () =>
    expect(round_trip_sentiment_positive_sync(Sentiment.Positive)).toBe(Sentiment.Positive));
  it("enums_round_trip_enums", () => {
    const e = new Enums({ bare_enum: Sentiment.Positive, variant_as_type: Sentiment.Positive });
    expect(round_trip_enums_sync(e)).toEqual(e);
  });
});
