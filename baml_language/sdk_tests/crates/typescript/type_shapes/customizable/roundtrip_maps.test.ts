// Roundtrip coverage for baml_sdk/maps — ported from test_maps.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Sentiment,
  Resume,
  MapContainer,
  round_trip_simple_map_sync,
  round_trip_enum_keyed_map_sync,
  round_trip_list_valued_map_sync,
  round_trip_sentiment_sync,
  round_trip_resume_sync,
  round_trip_map_container_sync,
} from "./baml_sdk/maps/index.js";

describe("roundtrip maps", () => {
  it("maps_round_trip_simple_map", () =>
    expect(round_trip_simple_map_sync({ a: 1, b: 2 })).toEqual({ a: 1, b: 2 }));
  it("maps_round_trip_enum_keyed_map", () => {
    const m: Parameters<typeof round_trip_enum_keyed_map_sync>[0] = {
      [Sentiment.Positive]: new Resume({ name: "up" }),
    };
    expect(round_trip_enum_keyed_map_sync(m)).toEqual(m);
  });
  it("maps_round_trip_list_valued_map", () =>
    expect(round_trip_list_valued_map_sync({ k: [1, 2] })).toEqual({ k: [1, 2] }));
  it("maps_round_trip_sentiment", () =>
    expect(round_trip_sentiment_sync(Sentiment.Positive)).toBe(Sentiment.Positive));
  it("maps_round_trip_resume", () => {
    const r = new Resume({ name: "n" });
    expect(round_trip_resume_sync(r)).toEqual(r);
  });
  it("maps_round_trip_map_container", () => {
    const enumKeyed: Parameters<typeof round_trip_enum_keyed_map_sync>[0] = {
      [Sentiment.Negative]: new Resume({ name: "dn" }),
    };
    const c = new MapContainer({
      simple: { a: 1 },
      enum_keyed: enumKeyed,
      list_valued: { k: [3] },
    });
    expect(round_trip_map_container_sync(c)).toEqual(c);
  });
});
