import "../baml_sdk/index.js";
import { runInNewContext } from "node:vm";
import { describe, expect, it } from "vitest";
import { hello_world_async } from "../baml_sdk/index.js";
import {
  GenericPair,
  identity,
  second_of,
} from "../baml_sdk/generic_tests/index.js";

describe("function_calls — TypeScript/Node-specific async smoke", () => {
  it("async hello_world returns the literal", async () => {
    await expect(hello_world_async()).resolves.toBe("hello world");
  });

  it("rejects cyclic input with the active property path", () => {
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;

    expect(() => identity(cyclic)).toThrow(
      /cyclic value at \$\.x\.self.*active value at \$\.x/i,
    );
  });

  it("allows a repeated object across acyclic sibling branches", () => {
    const shared = { n: 1 };
    expect(identity({ left: shared, right: shared })).toEqual({
      left: { n: 1 },
      right: { n: 1 },
    });
  });

  it("encodes a plain record from another JavaScript realm structurally", () => {
    const record = runInNewContext("({ count: 7, label: 'cross-realm' })") as {
      count: number;
      label: string;
    };

    // Its prototype belongs to node:vm, so identity against this realm's
    // Object.prototype is false even though it is still a plain record.
    expect(Object.getPrototypeOf(record)).not.toBe(Object.prototype);
    expect(identity(record)).toEqual({ count: 7, label: "cross-realm" });
  });

  it("infers missing generic class bindings from partial $types", () => {
    const pair = new GenericPair<number, string>({
      first: 1,
      second: "inferred",
      $types: { A: "int" },
    });

    expect(second_of(pair)).toBe("inferred");
  });
});
