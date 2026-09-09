#!/usr/bin/env -S uv run --script

# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "pyyaml==6.0.3",
#   "slack-sdk==3.41.0",
# ]
# ///

import json
import os
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any
from urllib.parse import urlencode
from zoneinfo import ZoneInfo

from slack_sdk import WebClient
from slack_sdk.errors import SlackApiError, SlackClientError

from bctl_src.oncall.current import current_oncall
from bctl_src.oncall.slack import email_for, lookup_user_id

GITHUB_API_TIMEOUT_SECONDS = 30
SLACK_SECTION_TEXT_LIMIT = 2800
SUCCESSFUL_JOB_CONCLUSIONS = {"success", "skipped"}


@dataclass(frozen=True)
class Failure:
    job_name: str
    job_url: str
    conclusion: str
    step_names: list[str]


def required_env(name: str) -> str:
    value = os.environ.get(name, "")
    if not value:
        raise RuntimeError(f"{name} is not set")
    return value


def next_page(link_header: str | None) -> str | None:
    if not link_header:
        return None
    for link in link_header.split(","):
        url, *parameters = link.strip().split(";")
        if any(parameter.strip() == 'rel="next"' for parameter in parameters):
            return url.strip("<>")
    return None


def get_json(url: str, token: str) -> tuple[dict[str, Any], str | None]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "baml-release-notifier",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urllib.request.urlopen(
            request, timeout=GITHUB_API_TIMEOUT_SECONDS
        ) as response:
            return json.load(response), response.headers.get("Link")
    except urllib.error.HTTPError as error:
        body = error.read().decode(errors="replace")
        raise RuntimeError(
            f"GitHub API request failed with HTTP {error.code}: {body}"
        ) from error


def find_failures(
    repository: str, run_id: str, run_attempt: str, token: str
) -> list[Failure]:
    url: str | None = (
        f"https://api.github.com/repos/{repository}/actions/runs/{run_id}"
        f"/attempts/{run_attempt}/jobs?per_page=100"
    )
    failures: list[Failure] = []
    while url:
        payload, link_header = get_json(url, token)
        for job in payload["jobs"]:
            conclusion = job.get("conclusion")
            failed_steps = [
                step["name"]
                for step in job.get("steps", [])
                if step.get("conclusion") == "failure"
            ]
            if (
                conclusion not in SUCCESSFUL_JOB_CONCLUSIONS and conclusion is not None
            ) or failed_steps:
                failures.append(
                    Failure(
                        job_name=job["name"],
                        job_url=job["html_url"],
                        conclusion=conclusion or "failure",
                        step_names=failed_steps,
                    )
                )
        url = next_page(link_header)
    return failures


def get_run_started_at(
    repository: str, run_id: str, run_attempt: str, token: str
) -> datetime:
    url = (
        f"https://api.github.com/repos/{repository}/actions/runs/{run_id}"
        f"/attempts/{run_attempt}"
    )
    payload, _ = get_json(url, token)
    return datetime.fromisoformat(payload["run_started_at"].replace("Z", "+00:00"))


def format_pacific_time(timestamp: datetime) -> str:
    pacific = timestamp.astimezone(ZoneInfo("America/Los_Angeles"))
    hour = pacific.strftime("%I").lstrip("0")
    am_pm = pacific.strftime("%p").lower()
    return f"{pacific.strftime('%b')} {pacific.day} {hour}:{pacific.strftime('%M')}{am_pm} PT"


def format_failure(failure: Failure) -> str:
    job = f"<{failure.job_url}|{failure.job_name}>"
    if failure.step_names:
        suffix = "" if len(failure.step_names) == 1 else "s"
        return f"• {job} — failed step{suffix}: {', '.join(failure.step_names)}"
    if failure.conclusion == "failure":
        return f"• {job} — job failed before a failed step was reported"
    conclusion = failure.conclusion.replace("_", " ")
    return f"• {job} — job concluded {conclusion}"


def notification_source_url(repository: str) -> str:
    workflow_path = (
        required_env("GITHUB_WORKFLOW_REF")
        .removeprefix(f"{repository}/")
        .rsplit("@", 1)[0]
    )
    # GitHub code search follows the repository's default branch (canary).
    query = f'repo:{repository} path:"{workflow_path}" "{Path(__file__).name}"'
    return f"https://github.com/search?{urlencode({'q': query, 'type': 'code'})}"


def current_oncall_mentions(slack_client: WebClient) -> list[str]:
    try:
        names = current_oncall()
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        print(f"Could not read current on-call schedule: {error}", file=sys.stderr)
        return []

    mentions = []
    for name in names:
        try:
            mentions.append(f"<@{lookup_user_id(slack_client, email_for(name))}>")
        except (SlackClientError, OSError, KeyError) as error:
            print(f"Could not look up on-call user {name}: {error}", file=sys.stderr)
    return mentions


def main() -> int:
    """Notify Slack of the current release result."""
    try:
        repository = required_env("GITHUB_REPOSITORY")
        run_id = required_env("GITHUB_RUN_ID")
        run_attempt = required_env("GITHUB_RUN_ATTEMPT")
        github_token = required_env("GH_TOKEN")
        slack_channel = required_env("SLACK_CHANNEL")
        slack_token = required_env("SLACK_BOT_TOKEN")
        slack_client = WebClient(token=slack_token)

        version = os.environ.get("VERSION") or "unknown version"
        channel = os.environ.get("CHANNEL") or "unknown channel"
        release_succeeded = os.environ.get("RELEASE_SUCCEEDED") == "true"
        started_at = get_run_started_at(repository, run_id, run_attempt, github_token)
        failures = find_failures(repository, run_id, run_attempt, github_token)

        run_url = (
            f"https://github.com/{repository}/actions/runs/{run_id}"
            f"/attempts/{run_attempt}"
        )
        if failures or not release_succeeded:
            mentions = current_oncall_mentions(slack_client)
            oncall_text = (
                f"\n\n{' '.join(mentions)} is current oncall, please investigate; "
                "see also <https://github.com/BoundaryML/baml/blob/canary/"
                "baml_language/RELEASING.md|RELEASING.md>."
                if mentions
                else ""
            )
            if failures:
                failure_text = "\n".join(
                    format_failure(failure) for failure in failures
                )
            else:
                failure_text = "• Required release completion gate did not succeed"
            message = (
                f"❌ BAML {channel} release failed: {version}, "
                f"started at {format_pacific_time(started_at)}{oncall_text}\n\n"
                f"*Failures:*\n{failure_text}"
            )
        else:
            message = (
                f"✅ BAML {channel} release succeeded: {version}, "
                f"started at {format_pacific_time(started_at)}"
            )

        blocks = [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": (
                        paragraph[: SLACK_SECTION_TEXT_LIMIT - 3] + "..."
                        if len(paragraph) > SLACK_SECTION_TEXT_LIMIT
                        else paragraph
                    ),
                },
            }
            for paragraph in message.split("\n\n")
        ]
        footer = (
            f"<{run_url}|View workflow run> · "
            f"<{notification_source_url(repository)}|View notification source>"
        )
        blocks.append(
            {
                "type": "context",
                "elements": [
                    {
                        "type": "mrkdwn",
                        "text": footer,
                    }
                ],
            }
        )

        slack_client.chat_postMessage(
            channel=slack_channel,
            text=f"{message}\n\n{footer}",
            blocks=blocks,
            unfurl_links=False,
        )
        return 0
    except SlackApiError as error:
        print(
            f"Slack API rejected the notification: {error.response.get('error', 'unknown_error')}",
            file=sys.stderr,
        )
    except SlackClientError as error:
        print(f"Slack request failed: {error}", file=sys.stderr)
    except (KeyError, RuntimeError, urllib.error.URLError) as error:
        print(f"Release notification failed: {error}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
