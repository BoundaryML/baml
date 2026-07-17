// Mirrors python_pydantic2/customizable/docstrings_etc/test_main.py.
// TypeScript does not expose JSDoc at runtime, so these assertions read
// the generated SDK source directly.

import { describe, it, expect } from "vitest";
import { Doc, Note, Priority, Sentiment } from "./baml_sdk/docs/index.js";
import { isTestRuntime } from "./test_runtime.js";

let readFileSync: typeof import("node:fs").readFileSync;
let join: typeof import("node:path").join;
if (isTestRuntime("node")) {
  ({ readFileSync } = await import("node:fs"));
  ({ join } = await import("node:path"));
}

const docsSource = () =>
  readFileSync(join(__dirname, "baml_sdk", "docs", "index.ts"), "utf8").replace(
    /\r\n/g,
    "\n",
  );

describe.runIf(isTestRuntime("node"))("docstrings_etc", () => {
  it("imports all documented symbols from baml_sdk/docs", () => {
    expect(Doc).toBeDefined();
    expect(Note).toBeDefined();
    expect(Priority).toBeDefined();
    expect(Sentiment).toBeDefined();
  });

  it("renders class summaries and Attributes sections as JSDoc", () => {
    const src = docsSource();

    expect(src).toContain(`/**
 * A document with a title and an optional body.
 *
 * Attributes:
 *   title: Title shown in lists and search results.
 *   body: Free-form body text.
 */
export class Doc {`);

    expect(src).toContain(`/**
 * A multi-line summary.
 * Continuation line of the summary, preserved verbatim in the
 * rendered block-form docstring.
 *
 * Attributes:
 *   id: Stable identifier — surfaces in URLs.
 *   text
 */
export class Note {`);
  });

  it("renders enum summaries and Members sections as JSDoc", () => {
    const src = docsSource();

    expect(src).toContain(`/**
 * Sentiment labels surfaced by the model.
 *
 * Members:
 *   HAPPY: Smiling face.
 *   SAD: Frowning face.
 *   NEUTRAL
 */
export enum Sentiment {`);

    expect(src).toContain(`/**
 * Pin the "summary only, no member rollup" case: this enum has a
 * class-level \`///\` but every variant is bare.
 */
export enum Priority {`);
  });

  it("does not emit inline field or variant doc artifacts", () => {
    const src = docsSource();

    expect(src).not.toContain("// Title shown in lists");
    expect(src).not.toContain("// Smiling face");
  });
});
