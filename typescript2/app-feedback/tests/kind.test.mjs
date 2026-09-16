import { expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { IssueDetail } from "../src/components/issues/issue-detail.tsx";
import { ISSUES } from "../src/lib/mock-data.ts";
import { investigationPrompt } from "../src/lib/investigation.ts";

test("a feature request leads with its kind and shows a proposed feature, a bug a resolution plan", () => {
  const bug = { ...ISSUES[0], kind: "bug", resolution_plan: "Bounds-check the lookup." };
  const feature = { ...ISSUES[0], kind: "feature", resolution_plan: "Add `baml.iter.take(n)`." };
  const bugHtml = renderToStaticMarkup(React.createElement(IssueDetail, { issue: bug }));
  const featureHtml = renderToStaticMarkup(React.createElement(IssueDetail, { issue: feature }));
  expect(bugHtml).toContain("Resolution plan (triage)");
  expect(bugHtml).not.toContain("Feature request");
  expect(featureHtml).toContain("Proposed feature");
  expect(featureHtml.indexOf("Feature request")).toBeLessThan(featureHtml.indexOf(feature.id));
  expect(investigationPrompt(feature)).toContain("feature request");
  expect(investigationPrompt(bug)).not.toContain("feature request");
});
