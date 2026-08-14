// Host-supplied json must materialize with `json` container typing.
//
// Inbound objects/arrays from the TypeScript bridge carry no element-type
// annotation on the wire; the engine must re-annotate them with the
// `baml.json.json` alias so typed narrowing inside BAML — `match (j) {
// let m: map<string, json> => ... }`, and therefore `baml.json.path` /
// `path_or` — treats them exactly like BAML-born `baml.json.parse` values.
import "./baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import {
  json_callback_kind_async,
  json_kind,
  json_path_string,
  json_path_string_or,
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
