// TypeScript/Node-only map coverage. Python's outbound decoder does not yet
// preserve typed enum map keys, so these cases stay outside the parity suite.
import "../baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  Sentiment,
  Resume,
  MapContainer,
  round_trip_enum_keyed_map,
  round_trip_map_container,
} from "../baml_sdk/maps/index.js";

describe("language-specific roundtrip maps", () => {
  it("test_round_trip_enum_keyed_map", () => {
    const m: Parameters<typeof round_trip_enum_keyed_map>[0] = {
      [Sentiment.Positive]: new Resume({ name: "up" }),
    };
    expect(round_trip_enum_keyed_map(m)).toEqual(m);
  });

  it("test_round_trip_map_container", () => {
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
