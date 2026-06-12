"""@bammy analytics route: natural language -> HogQL -> PostHog -> Slack.

"how many feedback events this week?" becomes a HogQL query written by a
claude-proxy agent (grounded with the project's top event names), runs
read-only against PostHog's query API, and the rows come back as a threaded
monospace table. One repair pass: if PostHog rejects the query, the agent
gets the exact error and rewrites once.
"""

from __future__ import annotations

import json
import logging
import os
import uuid
from typing import Any, Optional

from bench_core import posthog_client, slack_client
from bench_core.jsonl import extract_last_json_object
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import RunAgentRequest

log = logging.getLogger("uvicorn.error")

# SQL-shaped generation benefits from a stronger model than the router's.
HOGQL_MODEL = os.environ.get("BAMMY_HOGQL_MODEL", "claude-sonnet-4-6")
HOGQL_TIMEOUT_SECS = int(os.environ.get("BAMMY_HOGQL_TIMEOUT_SECS", "120"))

SYSTEM_PROMPT_HOGQL = """\
You translate one analytics question into a single HogQL query for PostHog.

HogQL is PostHog's ClickHouse-SQL dialect. The main table is `events` with
columns: event (string), timestamp (DateTime), distinct_id, person_id, and
properties (JSON; access keys as properties.foo or properties['foo'];
person properties as person.properties.foo). Useful forms:
  now() - INTERVAL 7 DAY, toDate(timestamp), count(), count(DISTINCT ...),
  countIf(cond), GROUP BY / ORDER BY / LIMIT.

Rules:
- Exactly one read-only SELECT. Never mutate anything.
- Always include a LIMIT (<= 100) and, unless the question says otherwise,
  a timestamp bound (default: the last 30 days) so the query stays cheap.
- Prefer event names from the provided catalog; if the question names an
  event that is not in the catalog, use it verbatim anyway.
- When asked for a trend over time, group by toDate(timestamp) ordered
  ascending.

Write your answer as a single JSON object to a file named `hogql.json`
(raw JSON, no markdown fences): {"hogql": "<the query>"}. Write the file
and stop.
"""


async def _nl_to_hogql(question: str, catalog: str, prior_error: Optional[str] = None) -> str:
    """Have a proxy agent write the HogQL for a question.

    Args:
        question: The user's analytics question.
        catalog: Rendered top-event names for grounding, or "".
        prior_error: PostHog's rejection message on the repair pass.

    Returns:
        The HogQL string.

    Raises:
        RuntimeError: When the agent run fails or emits no parseable query.
    """
    user = f"Question:\n{question.strip()}\n"
    if catalog:
        user += f"\nEvent catalog (most frequent first, last 30 days):\n{catalog}\n"
    if prior_error:
        user += (
            "\nYour previous query was rejected by PostHog with this error — "
            f"fix it:\n{prior_error}\n"
        )
    user += "\nWrite hogql.json now."

    proxy = ProxyClient.from_env()
    req = RunAgentRequest(
        cell_id=f"bammy-hogql-{uuid.uuid4().hex[:10]}",
        model=HOGQL_MODEL,
        max_turns=3,
        prompt=user,
        system_prompt=SYSTEM_PROMPT_HOGQL,
        post_file_patterns=["hogql.json"],
        invocation_timeout_secs=HOGQL_TIMEOUT_SECS,
    )
    result = await proxy.run_agent(req, timeout=HOGQL_TIMEOUT_SECS + 60)
    if result.status != "ok":
        raise RuntimeError(f"hogql agent run {result.status} (exit {result.exit_code})")
    raw = result.post_files.get("hogql.json")
    data: Any = None
    if raw:
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            data = None
    if not isinstance(data, dict):
        data = extract_last_json_object(result.transcript or "")
    query = (data or {}).get("hogql") if isinstance(data, dict) else None
    if not query or not isinstance(query, str):
        raise RuntimeError("hogql agent did not produce a parseable hogql.json")
    return query.strip()


async def handle(service: Any, bot_token: str, event: dict[str, Any],
                 intent: dict[str, Any]) -> None:
    """Answer one analytics question in-thread.

    Args:
        service: Service client (unused; kept for handler signature parity).
        bot_token: Slack bot token for replies.
        event: The Slack event (channel, ts, thread_ts).
        intent: The classifier's emit_route output (posthog_question).
    """
    channel = event.get("channel")
    thread = event.get("thread_ts") or event.get("ts")

    async def reply(text: str) -> None:
        await slack_client.post_message(bot_token, channel, text, thread_ts=thread)

    if not posthog_client.configured():
        await reply(
            "PostHog isn't wired up yet — add `ATB_POSTHOG_API_KEY` and "
            "`ATB_POSTHOG_PROJECT_ID` to Infisical and restart ingress."
        )
        return

    question = (intent.get("posthog_question") or "").strip() or (event.get("text") or "")
    catalog = "\n".join(f"- {name} ({n})" for name, n in await posthog_client.top_events())

    query = ""
    try:
        query = await _nl_to_hogql(question, catalog)
        try:
            out = await posthog_client.hogql(query)
        except RuntimeError as e:
            # One repair pass: the agent sees PostHog's exact rejection.
            query = await _nl_to_hogql(question, catalog, prior_error=str(e))
            out = await posthog_client.hogql(query)
    except Exception as e:  # noqa: BLE001 — report, never crash the router
        log.exception("bammy: posthog query failed")
        detail = str(e)[:300]
        await reply(f"Couldn't answer that: {detail}" + (f"\n```{query}```" if query else ""))
        return

    table = posthog_client.format_table(out["columns"], out["results"])
    await reply(f"```{query}```\n```{table}```")
    log.info("bammy: posthog query answered (%d rows)", len(out["results"]))
