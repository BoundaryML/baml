import { expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { ReproCard } from "../src/components/issues/repro-card.tsx";

const repro = {
  files: { "regression.baml": 'test "reject" { assert.equal(false, true); }' },
  command: "baml test", setup: null,
  expectation: { check: "should_not_compile", diagnostic_contains: null },
};

test("compiler source and actual diagnostic precede the collapsed test harness", () => {
  const html = renderToStaticMarkup(React.createElement(ReproCard, { repro: {
    ...repro, source_files: { "example.baml": "invalid source" },
    source_observed: "stderr: E0007 <script>alert(1)</script>",
    observed: "assertion failed", result: "fails",
  } }));
  expect(html.indexOf("example.baml")).toBeLessThan(html.indexOf("<details"));
  expect(html.indexOf("E0007")).toBeLessThan(html.indexOf("<details"));
  expect(html.indexOf("regression.baml")).toBeGreaterThan(html.indexOf("<details"));
  expect(html).not.toContain("<details open");
  expect(html).not.toContain("<script>");
  expect(html).toContain("assertion failed");
});

test("legacy repros remain visible without claiming they passed or failed", () => {
  const html = renderToStaticMarkup(React.createElement(ReproCard, { repro }));
  expect(html).toContain("regression.baml");
  expect(html).toContain("Result not classified");
  expect(html).toContain("No execution output recorded");
  expect(html).not.toContain("<details");
});

test("a passing comparison is distinguished from an unconfirmed example", () => {
  const html = renderToStaticMarkup(React.createElement(ReproCard, {
    repro: { ...repro, result: "passes", observed: "1 passed" }, comparison: true,
  }));
  expect(html).toContain("Comparison · Passes");
  expect(html).toContain("1 passed");
});
