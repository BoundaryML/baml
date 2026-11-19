// Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
// references: one passed back is the same instance, wherever a class can sit.
// Its state shows which instance a call reached: one hasher fed "a" and then
// "b" hashes as a fresh one fed "ab".
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { DefaultHasher } from "./baml_sdk/baml/hash/index.js";
import {
  HasherMarker,
  describe_hasher_or_marker_sync,
  hash_after_sync,
  hasher_or_marker_sync,
  hasher_through_callback,
  hasher_twice_sync,
  hasher_under_sync,
  hold_hasher_sync,
  make_hasher_sync,
  maybe_hasher_sync,
  same_hasher_sync,
} from "./baml_sdk/references/index.js";

describe("function_calls — references", () => {
  it("references_a_returned_instance_passed_back_is_the_same_instance", () => {
    const hasher = make_hasher_sync();
    hash_after_sync(hasher, "a");
    const fedTwice = hash_after_sync(hasher, "b");
    expect(fedTwice).toBe(hash_after_sync(make_hasher_sync(), "ab"));
    // Negative control: a fresh hasher fed only "b" differs.
    expect(fedTwice).not.toBe(hash_after_sync(make_hasher_sync(), "b"));
  });

  it("references_one_engine_instance_is_one_reference", () => {
    const hasher = make_hasher_sync();
    expect(hasher).toBeInstanceOf(DefaultHasher);
    expect(same_hasher_sync(hasher)).toBe(hasher);
    expect(make_hasher_sync()).not.toBe(hasher);
  });

  it("references_containers_hold_the_instance", () => {
    const hasher = make_hasher_sync();
    expect(maybe_hasher_sync(true)).toBeInstanceOf(DefaultHasher);
    expect(maybe_hasher_sync(false)).toBeNull();
    const twice = hasher_twice_sync(hasher);
    expect(twice).toHaveLength(2);
    expect(twice[0]).toBe(hasher);
    expect(twice[1]).toBe(hasher);
    expect(hasher_under_sync("key", hasher)["key"]).toBe(hasher);
  });

  it("references_a_field_holds_the_instance", () => {
    const hasher = make_hasher_sync();
    const holder = hold_hasher_sync("label", hasher);
    expect(holder.label).toBe("label");
    expect(holder.hasher).toBe(hasher);
  });

  it("references_a_union_selects_the_instance_arm", () => {
    expect(hasher_or_marker_sync(true)).toBeInstanceOf(DefaultHasher);
    expect(hasher_or_marker_sync(false)).toEqual(new HasherMarker({ tag: "marker" }));
    expect(describe_hasher_or_marker_sync(make_hasher_sync())).toBe("hasher");
    expect(describe_hasher_or_marker_sync(new HasherMarker({ tag: "marker" }))).toBe("marker");
  });

  it("references_a_callback_receives_and_returns_the_instance", async () => {
    const hasher = make_hasher_sync();
    const received: DefaultHasher[] = [];
    const callback = (value: DefaultHasher) => {
      received.push(value);
      return value;
    };
    await expect(hasher_through_callback(hasher, callback)).resolves.toBe(hasher);
    expect(received).toEqual([hasher]);
    expect(received[0]).toBe(hasher);
  });
});
