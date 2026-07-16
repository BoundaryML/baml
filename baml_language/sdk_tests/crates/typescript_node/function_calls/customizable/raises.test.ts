// TypeScript/Node counterpart to Python's test_raises.py. Generated .ts files
// are the declaration surface, so JSDoc source is the Node equivalent of
// Python runtime docstrings plus .pyi inspection.
import "./baml_sdk/index.js";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { raises_test } from "./baml_sdk/index.js";

const source = readFileSync(
  join(process.cwd(), "baml_sdk", "raises_test", "index.ts"),
  "utf8",
);

function jsDocBefore(marker: string): string {
  const markerIndex = source.indexOf(marker);
  expect(markerIndex, `missing generated marker ${marker}`).toBeGreaterThanOrEqual(0);
  const end = source.lastIndexOf("*/", markerIndex);
  const start = source.lastIndexOf("/**", end);
  expect(start, `missing JSDoc before ${marker}`).toBeGreaterThanOrEqual(0);
  expect(source.slice(end + 2, markerIndex).trim()).toBe("");
  return source.slice(start, end + 2);
}

describe("function_calls — throws JSDoc", () => {
  it("test_imports", () => {
    expect(raises_test.DocLoader).toBeTypeOf("function");
    expect(raises_test.InferredThrow).toBeTypeOf("function");
    expect(raises_test.LoadDoc).toBeTypeOf("function");
    expect(raises_test.PureLen).toBeTypeOf("function");
    expect(raises_test.Reparse).toBeTypeOf("function");
  });

  it("test_union_throws_lists_all_names", () => {
    const doc = jsDocBefore("export const LoadDoc =");

    expect(doc).toContain(" * @throws ParseError");
    expect(doc).toContain(" * @throws TimeoutError");
    expect(doc.indexOf("@throws ParseError")).toBeLessThan(
      doc.indexOf("@throws TimeoutError"),
    );
  });

  it("test_async_sibling_also_has_raises", () => {
    const doc = jsDocBefore("export const LoadDoc_async =");

    expect(doc).toContain(" * @throws ParseError");
    expect(doc).toContain(" * @throws TimeoutError");
  });

  it("test_single_throws", () => {
    const doc = jsDocBefore("export const Reparse =");

    expect(doc).toContain(" * @throws ParseError");
    expect(doc).not.toContain("@throws TimeoutError");
  });

  it("test_summary_precedes_raises_block", () => {
    const doc = jsDocBefore("export const LoadDoc =");

    expect(doc).toContain(" * Load a document from a path.");
    expect(doc.indexOf("Load a document from a path.")).toBeLessThan(
      doc.indexOf("@throws ParseError"),
    );
  });

  it("test_inferred_contract_without_clause_still_raises", () => {
    const doc = jsDocBefore("export const InferredThrow =");

    expect(doc).toContain(" * @throws ParseError");
  });

  it("test_non_throwing_function_has_no_raises_block", () => {
    const doc = jsDocBefore("export const PureLen =");

    expect(doc).not.toContain("@throws");
  });

  it("test_method_raises_block_in_pyi", () => {
    // Generated TypeScript is both runtime implementation and IDE/type surface,
    // so inspect the class methods in index.ts rather than a separate stub.
    const loadDoc = jsDocBefore('load = defineInstanceFunction("user.raises_test.DocLoader.load"');
    const createDoc = jsDocBefore(
      'static create = defineFunction("user.raises_test.DocLoader.create"',
    );

    expect(loadDoc).toContain(" * @throws ParseError");
    expect(createDoc).toContain(" * @throws TimeoutError");
  });
});
