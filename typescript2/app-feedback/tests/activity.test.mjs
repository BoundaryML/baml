import { expect, test } from "bun:test";
import { activityText, eventProposalPath } from "../src/lib/activity.ts";

test("issue and PR lifecycle render without exposing arbitrary payloads", () => {
  for (const kind of ["issue_created", "approved", "fix_started", "pr_opened", "babysit_started", "babysit_proposed", "babysit_approved", "babysit_pushing", "babysit_round", "babysit_result", "unknown"]) {
    const text = activityText({ kind, payload: { plan: "private-fixture", summary: "private-fixture", result: "private-fixture" } });
    expect(text).not.toContain("private-fixture");
    expect(text.length).toBeGreaterThan(10);
  }
});
test("green is monitoring, not automatic merge or claimed review approval", () => {
  expect(activityText({ kind: "babysit_result", payload: { result: "green" } })).toContain("Still watching");
  expect(activityText({ kind: "approved", payload: {} })).toContain("Shepherd approved");
});
test("proposal links reject control characters and external paths", () => {
  for (const id of ["../secret", "https://example.invalid", "x?next=bad", "x\n", ""]) {
    expect(eventProposalPath({ payload: { proposal_id: id } })).toBeNull();
  }
  expect(eventProposalPath({ payload: { proposal_id: "p-123" } })).toBe("/proposals/p-123");
});
