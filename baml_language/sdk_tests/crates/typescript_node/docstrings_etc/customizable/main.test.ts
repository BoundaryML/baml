// Mirrors python_pydantic2/customizable/docstrings_etc/test_main.py.
//
// Asserts on the runtime shape of the generated symbols for BAML
// `///` doc-comment lowering. TS doesn't expose docstrings at
// runtime the way Python's `__doc__` does, so the doc-rendering
// assertions from the python suite are deferred until we settle on a
// strategy (e.g. parsing the emitted .d.ts via the TypeScript
// compiler API). For now this file pins the import + enum-shape
// invariants — enough to keep tsc + jest red until
// `codegen_nodejs::to_source_code` is implemented.

import { describe, it, expect } from "@jest/globals";

import * as docs from "./baml_sdk/docs";
import { Doc, Note, Priority, Sentiment } from "./baml_sdk/docs";

describe("docstrings_etc", () => {
  it("imports all documented symbols from baml_sdk/docs", () => {
    expect(Doc).toBeDefined();
    expect(Note).toBeDefined();
    expect(Priority).toBeDefined();
    expect(Sentiment).toBeDefined();
  });

  it("Sentiment enum has expected members", () => {
    // Python: `{v.name for v in Sentiment} == {"HAPPY", "SAD", "NEUTRAL"}`
    const members = Object.keys(Sentiment).filter((k) => isNaN(Number(k)));
    expect(new Set(members)).toEqual(new Set(["HAPPY", "SAD", "NEUTRAL"]));
  });

  it("Priority enum has expected members", () => {
    const members = Object.keys(Priority).filter((k) => isNaN(Number(k)));
    expect(new Set(members)).toEqual(new Set(["HIGH", "MEDIUM", "LOW"]));
  });

  it("baml_sdk/docs module exports are non-empty", () => {
    expect(Object.keys(docs).length).toBeGreaterThan(0);
  });
});
