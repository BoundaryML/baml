// Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
// references: one passed back is the same instance, wherever a class can sit.
// Its state shows which instance a call reached: one hasher fed "a" and then
// "b" hashes as a fresh one fed "ab".
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { DefaultHasher } from "./baml_sdk/baml/hash/index.js";
import {
  HasherMarker,
  describe_hasher_or_marker,
  hash_after,
  hasher_or_marker,
  hasher_through_callback_async,
  hasher_twice,
  hasher_under,
  hold_hasher,
  make_hasher,
  maybe_hasher,
  same_hasher,
} from "./baml_sdk/references/index.js";

describe("function_calls — references", () => {
  it("references_a_returned_instance_passed_back_is_the_same_instance", () => {
    const hasher = make_hasher();
    hash_after(hasher, "a");
    const fedTwice = hash_after(hasher, "b");
    expect(fedTwice).toBe(hash_after(make_hasher(), "ab"));
    // Negative control: a fresh hasher fed only "b" differs.
    expect(fedTwice).not.toBe(hash_after(make_hasher(), "b"));
  });

  it("references_one_engine_instance_is_one_reference", () => {
    const hasher = make_hasher();
    expect(hasher).toBeInstanceOf(DefaultHasher);
    expect(same_hasher(hasher)).toBe(hasher);
    expect(make_hasher()).not.toBe(hasher);
  });

  it("references_containers_hold_the_instance", () => {
    const hasher = make_hasher();
    expect(maybe_hasher(true)).toBeInstanceOf(DefaultHasher);
    expect(maybe_hasher(false)).toBeNull();
    const twice = hasher_twice(hasher);
    expect(twice).toHaveLength(2);
    expect(twice[0]).toBe(hasher);
    expect(twice[1]).toBe(hasher);
    expect(hasher_under("key", hasher)["key"]).toBe(hasher);
  });

  it("references_a_field_holds_the_instance", () => {
    const hasher = make_hasher();
    const holder = hold_hasher("label", hasher);
    expect(holder.label).toBe("label");
    expect(holder.hasher).toBe(hasher);
  });

  it("references_a_union_selects_the_instance_arm", () => {
    expect(hasher_or_marker(true)).toBeInstanceOf(DefaultHasher);
    expect(hasher_or_marker(false)).toEqual(new HasherMarker({ tag: "marker" }));
    expect(describe_hasher_or_marker(make_hasher())).toBe("hasher");
    expect(describe_hasher_or_marker(new HasherMarker({ tag: "marker" }))).toBe("marker");
  });

  it("references_a_callback_receives_and_returns_the_instance", async () => {
    const hasher = make_hasher();
    const received: DefaultHasher[] = [];
    const callback = (value: DefaultHasher) => {
      received.push(value);
      return value;
    };
    await expect(hasher_through_callback_async(hasher, callback)).resolves.toBe(hasher);
    expect(received).toEqual([hasher]);
    expect(received[0]).toBe(hasher);
  });
});
