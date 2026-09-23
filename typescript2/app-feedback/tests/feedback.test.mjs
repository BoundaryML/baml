import { expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { FeedbackList, fateOf } from "../src/components/feedback-list.tsx";

const issue = { id: "ISSUE-1", kind: "bug", title: "host callable returning a whole number for a float panics", description: "", shepherd: null, subsystem: "Runtime", repros: [], version: "0.19.0", feedback_ids: ["FB-min-4901"], status: { state: "open" }, comments: [], resolution_plan: null, difficulty: null, design_doc: null, outcome: null, dataset: "live", created_at: "2026-09-17T18:40:00Z", updated_at: "2026-09-17T18:40:00Z" };
const base = { source: "BamlFeedback", toolchain: "0.19.0", received_at: "2026-09-17T18:37:45Z", dataset: "live", body: "Node bridge 0.19.0. My JS host function returns 612 and the call panics." };
const reports = [
  { ...base, id: "FB-min-4901", title: "host callable panics", issue_ids: ["ISSUE-1"], issues: [issue], no_issue: null },
  { ...base, id: "FB-min-4765", title: "escapes kept raw", issue_ids: [], issues: [], no_issue: "fixed on this toolchain" },
  { ...base, id: "FB-min-4620", title: "openai compatible host", issue_ids: [], issues: [], no_issue: null, dataset: "eval" },
];

test("each report shows what it became: the issue, the no-issue reason, or that it is waiting", () => {
  const html = renderToStaticMarkup(React.createElement(FeedbackList, { reports }));
  expect(html).toContain('href="/feedback/FB-min-4901"');
  expect(html).toContain('href="/issues/ISSUE-1"');
  expect(html).toContain("No issue:");
  expect(html).toContain("fixed on this toolchain");
  expect(html).toContain("Waiting for triage.");
  expect(html).toContain("3 reports · 1 became an issue · 1 no issue · 1 waiting for triage");
  expect(html).toContain(">eval<");
  expect(html).not.toContain("device_id");
});

test("fateOf prefers an issue over a recorded reason", () => {
  expect(fateOf({ issues: [issue], no_issue: "stale" })).toBe("issue");
  expect(fateOf({ issues: [], no_issue: "needs_details" })).toBe("no_issue");
  expect(fateOf({ issues: [], no_issue: null })).toBe("pending");
});

test("an empty store renders a sentence, not an empty list", () => {
  expect(renderToStaticMarkup(React.createElement(FeedbackList, { reports: [] }))).toContain("No reports yet.");
});
