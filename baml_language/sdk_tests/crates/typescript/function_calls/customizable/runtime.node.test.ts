import "./baml_sdk/index.js";
import { describe, expect, it } from "vitest";

describe("Node runtime selection", () => {
  it("executes the generated SDK in Node", () => {
    expect(process.release.name).toBe("node");
    expect(typeof (globalThis as { document?: unknown }).document).toBe("undefined");
  });
});
