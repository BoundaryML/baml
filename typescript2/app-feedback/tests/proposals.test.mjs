import { expect, test } from "bun:test";
import { proposalFromEvents, validProposalId } from "../src/lib/proposals.ts";
import { eventProposalPath } from "../src/lib/activity.ts";

test("real BAML identifiers route; query injection is rejected", () => {
  const id = "baml_id_1_2LpwBxqmgcGF4L8B5Fs6rA";
  expect(validProposalId(id)).toBe(true);
  expect(eventProposalPath({ payload: { proposal_id: id } })).toBe(`/proposals/${id}`);
  for (const invalid of ["x&dataset=eq.eval", "../x", "x\n", "x?y", "a".repeat(81)]) expect(validProposalId(invalid)).toBe(false);
});
test("only the deliberately public summary is displayed", () => {
  const result = proposalFromEvents([{ kind: "babysit_proposed", payload: {
    proposed_fix: "Pin the image and test the broker.", plan: "private-plan", logs: "private-log",
  } }]);
  expect(result.fix).toBe("Pin the image and test the broker.");
  expect(JSON.stringify(result)).not.toContain("private-");
  expect(proposalFromEvents([{ kind: "babysit_proposed", payload: { plan: "private-plan" } }])).toBeNull();
});
test("Slack link targets the exact validated message on the configured workspace", () => {
  const original = process.env.FEEDBACK_SLACK_URL;
  process.env.FEEDBACK_SLACK_URL = "https://example.slack.com/archives/C123";
  try {
    const events = [{ kind: "babysit_proposed", payload: { proposed_fix: "Fix" } },
      { kind: "babysit_proposal_posted", payload: { channel: "C456", message_ts: "1788809693.106609" } }];
    expect(proposalFromEvents(events).slackUrl).toBe("https://example.slack.com/archives/C456/p1788809693106609");
    events[1].payload.channel = "https://evil.invalid";
    expect(proposalFromEvents(events).slackUrl).toBe("https://example.slack.com/archives/C123");
  } finally {
    if (original === undefined) delete process.env.FEEDBACK_SLACK_URL;
    else process.env.FEEDBACK_SLACK_URL = original;
  }
});
