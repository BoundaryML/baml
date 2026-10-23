// Host-supplied json must materialize with `json` container typing.
//
// Inbound objects/arrays from the TypeScript bridge carry no element-type
// annotation on the wire; the engine must re-annotate them with the
// `baml.json.json` alias so typed narrowing inside BAML — `match (j) {
// let m: map<string, json> => ... }`, and therefore `baml.json.path` /
// `path_or` — treats them exactly like BAML-born `baml.json.parse` values.
import "./baml_sdk/index.js";
import { BamlPanic } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import type { json } from "./baml_sdk/baml/json/index.js";
import {
  call_json_callback_async,
  json_callback_kind_async,
  json_kind,
  json_path_string,
  json_path_string_or,
  round_trip_json,
  round_trip_json_async,
} from "./baml_sdk/go_json_tests/index.js";

describe("function_calls — host-supplied json typed narrowing", () => {
  const object = {
    type: "ok",
    nested: { list: [1, { deep: "found" }] },
  };

  it("host_supplied_json_supports_typed_narrowing", () => {
    expect(json_kind(object)).toBe("object");
    expect(json_kind([1])).toBe("array");
    expect(json_kind("text")).toBe("string");
    expect(json_kind(3)).toBe("other");

    expect(json_path_string(object, ".type")).toBe("ok");
    expect(json_path_string(object, ".nested.list[1].deep")).toBe("found");
    expect(json_path_string_or(object, ".missing", "fallback")).toBe("fallback");

    expect(() => json_path_string(object, ".absent")).toThrow(/missing field/);
  });

  it("json_returned_from_host_callback_supports_typed_narrowing", async () => {
    // json returned from a host callback converts on the host-return path
    // (no argument coercion pass); it must narrow identically.
    await expect(
      json_callback_kind_async((value) => ({ wrapped: value }), "payload"),
    ).resolves.toBe("object");
  });
});

/** A json value whose lists nest `depth` deep around a `1`. */
function nestedJson(depth: number): json {
  let value: json = 1;
  for (let level = 0; level < depth; level += 1) value = [value];
  return value;
}

// The wire carries 100 nested messages and a json list level costs two, so a
// value nested 30 deep fits and one nested 60 deep does not. The bridge
// measures a value before sending it and refuses one the engine could not read.
describe("function_calls — json nested deeper than the wire carries", () => {
  it("json_nested_deeper_than_the_wire_carries_is_refused_as_an_argument", async () => {
    // Well inside the bound, the value crosses both ways.
    expect(round_trip_json(nestedJson(30))).toEqual(nestedJson(30));
    await expect(round_trip_json_async(nestedJson(30))).resolves.toEqual(nestedJson(30));
    // Just past it, and far past it: refused with the error any argument the
    // bridge cannot encode raises, before anything is sent.
    for (const depth of [60, 1000]) {
      expect(() => round_trip_json(nestedJson(depth))).toThrow(
        /round_trip_json: argument `value`.*nested too deeply for the boundary/,
      );
      await expect(round_trip_json_async(nestedJson(depth))).rejects.toThrow(
        /round_trip_json: argument `value`.*nested too deeply for the boundary/,
      );
    }
  });

  it("json_nested_deeper_than_the_wire_carries_is_refused_as_a_callback_result", async () => {
    // Well inside the bound, the callback's result crosses back into BAML.
    await expect(
      call_json_callback_async(() => nestedJson(30), 1),
    ).resolves.toEqual(nestedJson(30));
    // Past it, the bridge refuses the result rather than sending it, and the
    // call fails with the engine's panic for a broken callback contract.
    for (const depth of [60, 200]) {
      try {
        await call_json_callback_async(() => nestedJson(depth), 1);
        expect.unreachable(`a callback result nested ${depth} deep crossed`);
      } catch (caught) {
        expect(caught).toBeInstanceOf(BamlPanic);
        expect((caught as BamlPanic).className).toMatch(/^baml\.panics\./);
        expect((caught as BamlPanic).message).toMatch(/result.*nested too deeply for the boundary/);
      }
    }
  });
});
