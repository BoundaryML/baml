import { expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { Activity } from "../src/components/issues/activity.tsx";
import { IntuitionCard } from "../src/components/intuition-card.tsx";

const at = "2026-09-11T19:34:48Z";

test("a decision renders its step, decision, reason and collapsed evidence; unknown kinds are named", () => {
  const html = renderToStaticMarkup(React.createElement(Activity, { events: [
    { id: 1, kind: "gauged", slack_ts: null, created_at: at, payload: { step: "Gauge difficulty", decision: "Hard: the cause is unknown", reason: "Two subsystems <script>x</script>", evidence: { unknowns: ["which pass drops the entry"] } } },
    { id: 2, kind: "repro_verified", slack_ts: null, created_at: at, payload: { summary: "legacy" } },
    { id: 3, kind: "comment_synced", slack_ts: null, created_at: at, payload: { author: "octo-cat", summary: "Still broken on today's nightly", url: "https://github.com/BoundaryML/baml/issues/4790#issuecomment-12" } },
    { id: 4, kind: "comment_synced", slack_ts: null, created_at: at, payload: { author: "x", summary: "bad link", url: "https://evil.example/issues/1#issuecomment-1" } },
  ], issueId: "ISSUE-1" }));
  expect(html).toContain("Difficulty");
  expect(html).toContain("Hard");
  expect(html).toContain("which pass drops the entry");
  expect(html).not.toContain("<script>");
  expect(html).toContain("<details");
  expect(html).not.toContain("Issue status updated");
  expect(html).toContain("Repro verified on the latest nightly");
  expect(html).toContain("https://github.com/BoundaryML/baml/issues/4790#issuecomment-12");
  expect(html).not.toContain("evil.example");
});

test("an intuition card links every cited issue except the current one", () => {
  const html = renderToStaticMarkup(React.createElement(IntuitionCard, { current: "ISSUE-a", intuition: {
    id: "INT-1", title: "bigint is half-wired into the VM", kind: "SharedCause", insight: "Three tickets in one week.", evidence: "ISSUE-a, ISSUE-b",
    issue_ids: ["ISSUE-a", "ISSUE-b"], subsystem: "Runtime", confidence: "high", suggested_action: "Audit every bigint arm.", generated_at: at,
  } }));
  expect(html).toContain("/issues/ISSUE-b");
  expect(html).not.toContain("/issues/ISSUE-a");
  expect(html).toContain("high confidence");
  expect(html).toContain("Also cites");
});

test("the trail groups steps by phase, folds routine notifications and shows durations", () => {
  const at = (m) => `2026-09-15T00:${String(m).padStart(2, "0")}:00Z`;
  const html = renderToStaticMarkup(React.createElement(Activity, { events: [
    { id: 1, kind: "ingested", slack_ts: null, created_at: at(0), payload: { source: "Github" } },
    { id: 2, kind: "enrich_started", slack_ts: null, created_at: at(1), payload: { summary: "Checking" } },
    { id: 3, kind: "decision", slack_ts: null, created_at: at(2), payload: { step: "Assess report", decision: "Actionable bug", reason: "Concrete repro.", evidence: {} } },
    { id: 4, kind: "decision", slack_ts: null, created_at: at(14), payload: { step: "Write the ticket", decision: "Subsystem Runtime: x", reason: "VM output.", evidence: {} } },
    { id: 5, kind: "gauged", slack_ts: null, created_at: at(15), payload: { step: "Gauge difficulty", decision: "Medium: a few days", reason: "Known cause.", evidence: { unknowns: [] } } },
    { id: 6, kind: "slack_notified", slack_ts: null, created_at: at(16), payload: { summary: "Slack" } },
  ] }));
  for (const label of ["Report", "Triage", "Routing"]) expect(html).toContain(label);
  expect(html).toContain("12 min");
  expect(html).toContain("2 routine notifications folded");
  expect(html).not.toContain("Checking");
  expect(html).toContain("Assess report");
});

test("only the latest triage attempt is shown; earlier aborted attempts are folded", () => {
  const at = (m) => `2026-09-15T00:${String(m).padStart(2, "0")}:00Z`;
  const step = (id, m, step, decision) => ({ id, kind: "decision", slack_ts: null, created_at: at(m), payload: { step, decision, reason: "r", evidence: {} } });
  const html = renderToStaticMarkup(React.createElement(Activity, { events: [
    { id: 1, kind: "ingested", slack_ts: null, created_at: at(0), payload: { source: "Github" } },
    step(2, 1, "Assess report", "first attempt"), step(3, 5, "Write the ticket", "first ticket"),
    step(4, 10, "Assess report", "second attempt"), step(5, 14, "Write the ticket", "second ticket"), step(6, 15, "Check for duplicates", "New issue"),
    { id: 7, kind: "gauged", slack_ts: null, created_at: at(16), payload: { step: "Gauge difficulty", decision: "Easy", reason: "r", evidence: {} } },
  ] }));
  expect(html).toContain("second ticket");
  expect(html).not.toContain("first ticket");
  expect(html).toContain("1 earlier triage attempt hidden");
  expect(html).toContain("Report ingested");
  expect(html).toContain("Difficulty");
});

test("only the latest fix attempt is shown; earlier stopped attempts are counted", () => {
  const at = (m) => `2026-09-15T01:${String(m).padStart(2, "0")}:00Z`;
  const ev = (id, m, kind, payload = {}) => ({ id, kind, slack_ts: null, created_at: at(m), payload });
  const html = renderToStaticMarkup(React.createElement(Activity, { events: [
    ev(1, 0, "fix_started"), ev(2, 5, "needs_human", { summary: "first stop" }),
    ev(3, 10, "fix_started"), ev(4, 15, "needs_human", { summary: "second stop" }),
    ev(5, 20, "fix_started"), ev(6, 40, "pr_opened", { pr: "https://github.com/BoundaryML/baml/pull/4884" }),
  ] }));
  expect(html).toContain("pull/4884");
  expect(html).not.toContain("first stop");
  expect(html).not.toContain("second stop");
  expect(html).toContain("2 earlier fix attempts hidden");
});
