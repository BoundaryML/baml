import { expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { IssueDetail } from "../src/components/issues/issue-detail.tsx";
import { investigationPrompt } from "../src/lib/investigation.ts";

test("a feature request leads with its kind and shows a proposed feature, a bug a resolution plan", () => {
  const base = { id: "ISSUE-1", title: "throws clause with an unresolved type panics", description: "A `throws` clause naming a missing type crashes the compiler.", shepherd: "codeshaunted", subsystem: "Compiler", repros: [], version: "0.17.0", feedback_ids: ["FB-1"], status: { state: "open" }, comments: [], resolution_plan: null, difficulty: "Easy", design_doc: null, outcome: null, dataset: "live", created_at: "2026-08-27T09:12:00Z", updated_at: "2026-08-29T18:20:00Z" };
const bug = { ...base, kind: "bug", resolution_plan: "Bounds-check the lookup." };
  const feature = { ...base, kind: "feature", resolution_plan: "Add `baml.iter.take(n)`." };
  const bugHtml = renderToStaticMarkup(React.createElement(IssueDetail, { issue: bug }));
  const featureHtml = renderToStaticMarkup(React.createElement(IssueDetail, { issue: feature }));
  expect(bugHtml).toContain("Resolution plan");
  expect(bugHtml).not.toContain("Feature request");
  expect(featureHtml).toContain("Proposed feature");
  expect(featureHtml.indexOf("Feature request")).toBeLessThan(featureHtml.indexOf(feature.id));
  expect(investigationPrompt(feature)).toContain("feature request");
  expect(investigationPrompt(bug)).not.toContain("feature request");
});
