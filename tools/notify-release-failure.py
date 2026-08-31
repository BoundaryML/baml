#!/usr/bin/env -S uv run --script

# /// script
# requires-python = ">=3.12"
# dependencies = [
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
from typing import Any
from zoneinfo import ZoneInfo

from slack_sdk import WebClient
from slack_sdk.errors import SlackApiError, SlackClientError

GITHUB_API_TIMEOUT_SECONDS = 30
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


def main() -> int:
    """Notify Slack of the current release result."""
    try:
        repository = required_env("GITHUB_REPOSITORY")
        run_id = required_env("GITHUB_RUN_ID")
        run_attempt = required_env("GITHUB_RUN_ATTEMPT")
        github_token = required_env("GH_TOKEN")
        slack_channel = required_env("SLACK_CHANNEL")
        slack_token = required_env("SLACK_BOT_TOKEN")

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
            if failures:
                failure_text = "\n".join(
                    format_failure(failure) for failure in failures
                )
            else:
                failure_text = "• Required release completion gate did not succeed"
            message = (
                f"❌ BAML {channel} release failed: {version}, "
                f"started at {format_pacific_time(started_at)}\n\n"
                f"*Failures:*\n{failure_text}\n\n"
                f"*Run:* <{run_url}|View workflow run>"
            )
        else:
            message = (
                f"✅ BAML {channel} release succeeded: {version}, "
                f"started at {format_pacific_time(started_at)}\n\n"
                f"*Run:* <{run_url}|View workflow run>"
            )

        WebClient(token=slack_token).chat_postMessage(
            channel=slack_channel,
            text=message,
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
