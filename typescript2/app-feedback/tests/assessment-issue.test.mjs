import { expect, test } from "bun:test";
import { assessmentIssue } from "../src/lib/assessment-issue.ts";
const event = (text, continuation = false) => ({ role: "user", type: "text", text, continuation });

test("assessment navigation uses its ticket header, not referenced IDs", () => {
  expect(assessmentIssue([event("Issue ID: ISSUE-target\nTitle: Correct title\nDescription:\nGH-other ISSUE-other")])).toEqual({ id: "ISSUE-target", title: "Correct title" });
  expect(assessmentIssue([event("Description:\nIssue ID: ISSUE-unrelated")])).toBeNull();
  expect(assessmentIssue([event("Referenced ISSUE-unrelated")])).toBeNull();
});

test("older chunked transcripts recover the header without reading later conversation", () => {
  expect(assessmentIssue([event("Issue ID: GH-tar"), event("get\nTitle: Original title\nDescription:\n", true), event("Issue ID: ISSUE-other\nTitle: Other")])).toEqual({ id: "GH-target", title: "Original title" });
  expect(assessmentIssue([])).toBeNull();
});
