// The walker reads a container's children one at a time under one budget per
// call; a host-reported `length` sizes nothing and an array's iterator is
// never consulted. A sparse array of any length is therefore refused at the
// budget after reading that many holes, never copied into millions of slots
// first, and an array whose iterator never ends encodes what its indices
// hold.
import initWasm, * as raw from "#bridge-web-core";
import { beforeAll, describe, expect, it } from "vitest";

beforeAll(async () => {
  await initWasm();
  // Importing the runtime registers its host adapter with the bridge.
  await import("@boundaryml/baml-bridge-web");
});

describe("reader bounds", () => {
  it("refuses a sparse array far beyond the node budget at the budget, not by copying it", () => {
    const huge = new Array(2 ** 24); // 16,777,216 holes; the budget is 1,048,576 nodes
    const started = Date.now();
    expect(() => raw._encodeWebInbound(huge, "value")).toThrow(/more than 1048576 nodes/);
    // Reading a million holes is cheap; copying sixteen million is not.
    expect(Date.now() - started).toBeLessThan(10_000);
  });

  it("reads an array by its indices, never through its iterator", () => {
    const endless = [1, 2, 3];
    endless[Symbol.iterator] = function* () {
      while (true) yield 1;
    };
    // `Array.from` would never return; the indexed read encodes three items.
    expect(raw._encodeWebInbound(endless, "value")).toBeTruthy();
  });

  it("reads an array whose length lies through a Proxy by what it yields", () => {
    const lying = new Proxy([1, 2, 3], {
      get(target, property, receiver) {
        if (property === "length") return 1_000_000_000;
        return Reflect.get(target, property, receiver);
      },
    });
    expect(() => raw._encodeWebInbound(lying, "value")).toThrow(/more than 1048576 nodes/);
  });
});
