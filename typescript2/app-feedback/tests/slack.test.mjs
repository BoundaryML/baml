import { expect, test } from "bun:test";
import { slackChannelUrl } from "../src/lib/slack.ts";

test("Slack links reject external hosts, credentials, and redirects", () => {
  for (const url of [
    "javascript:alert(1)", "http://team.slack.com/archives/C123",
    "https://team.slack.com.attacker.invalid/archives/C123",
    "https://attacker.invalid/archives/C123", "https://slack.com/redirect",
    "https://user:password@team.slack.com/archives/C123",
    "https://team.slack.com:8443/archives/C123",
    "https://team.slack.com/archives/C123?redirect=https://attacker.invalid",
    "https://team.slack.com/archives/C123#redirect", "", "not-a-url",
  ]) expect(slackChannelUrl(url)).toBeNull();
  expect(slackChannelUrl("https://team.slack.com/archives/C123"))
    .toBe("https://team.slack.com/archives/C123");
});
