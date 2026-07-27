// Roundtrip coverage for baml_sdk/maps — ported from test_maps.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Sentiment,
  Resume,
  MapContainer,
  round_trip_simple_map,
  round_trip_enum_keyed_map,
  round_trip_list_valued_map,
  round_trip_sentiment,
  round_trip_resume,
  round_trip_map_container,
} from "./baml_sdk/maps/index.js";

describe("roundtrip maps", () => {
  it("maps_round_trip_simple_map", () =>
    expect(round_trip_simple_map({ a: 1, b: 2 })).toEqual({ a: 1, b: 2 }));
  it("maps_round_trip_enum_keyed_map", () => {
    const m: Parameters<typeof round_trip_enum_keyed_map>[0] = {
      [Sentiment.Positive]: new Resume({ name: "up" }),
    };
    expect(round_trip_enum_keyed_map(m)).toEqual(m);
  });
  it("maps_round_trip_list_valued_map", () =>
    expect(round_trip_list_valued_map({ k: [1, 2] })).toEqual({ k: [1, 2] }));
  it("maps_round_trip_sentiment", () =>
    expect(round_trip_sentiment(Sentiment.Positive)).toBe(Sentiment.Positive));
  it("maps_round_trip_resume", () => {
    const r = new Resume({ name: "n" });
    expect(round_trip_resume(r)).toEqual(r);
  });
  it("maps_round_trip_map_container", () => {
    const enumKeyed: Parameters<typeof round_trip_enum_keyed_map>[0] = {
      [Sentiment.Negative]: new Resume({ name: "dn" }),
    };
    const c = new MapContainer({
      simple: { a: 1 },
      enum_keyed: enumKeyed,
      list_valued: { k: [3] },
    });
    expect(round_trip_map_container(c)).toEqual(c);
  });
});
