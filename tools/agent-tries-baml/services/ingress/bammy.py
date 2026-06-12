"""@bammy — the unified Slack mention router.

One Slack app fronts the whole monolith. Every app_mention lands here (off
the request path, after ingress has ACKed) and is routed:

  1. Directive fast-path: any explicit bench directive (`[baml=N]`,
     `[canary]`, `[skill arena]`, ...) routes straight to the bench handler
     with zero model calls — existing muscle memory keeps working.
  2. Thread context: a mid-thread mention fetches the prior messages
     (conversations.replies) and passes them to the classifier AND the
     chosen handler, so "fix the title" can refer to the thread above.
  3. Intent classifier: a small agent call (Haiku) labels the message
     bench | changelog_edit | changelog_sync | promo_claim | posthog_query |
     feedback | general.
  4. Dispatch to the matching handler (services/ingress/handlers/*).

The classifier runs through claude-proxy (the same model path every worker
uses, billed via the proxy's Claude session). Fail-open design: with no
CLAUDE_PROXY_URLS configured (tests, local dev) or on any classifier error,
the mention falls back to the bench path — exactly the pre-bammy behavior
of this bot.
"""

from __future__ import annotations

import json
import logging
import os
import uuid
from typing import Any, Optional

from bench_core import slack_client
from bench_core.jsonl import extract_last_json_object
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import RunAgentRequest
from bench_core.service_client import ServiceClient

from .handlers import bench, changelog_edit, feedback, posthog_query, promo

log = logging.getLogger("uvicorn.error")

# Intent classification is a small labeling task — a fast model is plenty.
# (Must be on claude-proxy's ALLOWED_MODELS list — aliases, not dated ids.)
INTENT_MODEL = os.environ.get("BAMMY_INTENT_MODEL", "claude-haiku-4-5")
THREAD_CONTEXT_LIMIT = int(os.environ.get("BAMMY_THREAD_LIMIT", "30"))
CLASSIFY_TIMEOUT_SECS = int(os.environ.get("BAMMY_CLASSIFY_TIMEOUT_SECS", "120"))


def _bot_token() -> str:
    return os.environ.get("ATB_SLACK_BOT_TOKEN", "")


def _changelog_allowlist() -> set[str]:
    raw = os.environ.get("ATB_SLACK_CHANGELOG_ALLOWLIST", "")
    return {u.strip() for u in raw.split(",") if u.strip()}


SYSTEM_PROMPT_ROUTE = """\
You are the router for @bammy, the BAML team's Slack bot. Read one mention
(and, when given, the thread messages that came before it) and decide which of
the bot's capabilities the user wants. Write your decision as a single JSON
object to a file named `route.json` (raw JSON, no markdown fences) with
exactly these fields:
{"intent": "bench"|"changelog_edit"|"changelog_sync"|"promo_claim"|"feedback"|"posthog_query"|"general",
 "bench_prompt": str, "version_ref": str, "mode": "revise"|"regenerate",
 "guidance": str, "promo_notes": str, "feedback_text": str, "posthog_question": str,
 "needs_clarification": bool, "clarification_question": str}
Write the file and stop.

ROUTES
======
- `bench` -> run a benchmark/agent task: "try building X with baml",
  "run a bench on Y", "see how the agent handles Z". This is the DEFAULT when
  the user asks the bot to attempt, test, or build something with BAML.
  Explicit bench directives like [canary], [nightly], [baml=N], [coldstart],
  [skill arena] always mean bench (they are normally fast-pathed before you,
  but treat them as bench if you see them). Put the task in `bench_prompt`.
- `changelog_edit` -> change an existing changelog entry: "fix the title of
  0.222.0", "regenerate the latest nightly", "redo the entry but mention the
  new --no-cache flag". Fill `version_ref` (copied as the user said it, do not
  normalize), `mode` (`revise` unless they explicitly ask to redo from
  scratch), and `guidance` (the instruction restated cleanly; empty for
  regenerate). If the thread above is about a specific entry and the user says
  "fix the title", take the version from the thread.
- `changelog_sync` -> catch the changelog up with GitHub, no specific version:
  "add new changelog entries", "update the changelog", "pull the latest
  releases", "the cron missed a release, sync it". This enqueues generation
  for every release that has no entry yet. No other fields needed.
- `promo_claim` -> they want a t-shirt / promo / discount code: "promo code
  for Jane", "can I get a shirt code". Put any who/why detail in `promo_notes`.
- `posthog_query` -> an analytics/data question to answer from PostHog: "how
  many signups this week", "query posthog for the top events yesterday",
  "how many users ran baml fmt". Restate the question cleanly in
  `posthog_question` (keep any timeframe / event names the user gave).
- `feedback` -> they are reporting an experience with BAML itself (a bug, a
  papercut, praise, a confusing error) and want it logged, not acted on right
  now: "feedback: baml fmt is slow on big files". Restate it in `feedback_text`.
- `general` -> greetings, questions about what the bot can do, or anything
  that fits no route. The bot will reply with its capabilities.

CLARIFICATION
=============
Set `needs_clarification` true (with one short `clarification_question`) only
when the route is clear but unusable — e.g. changelog_edit with no
identifiable version anywhere in the message or thread. Ambiguity between
routes resolves to the most likely route, not a clarification.

Fill every required field; use empty strings for the fields your route does
not use.
"""


async def _classify(text: str, thread_context: str, catalog: str) -> dict[str, Any]:
    """Run the intent classification through claude-proxy.

    Args:
        text: The mention text (bot mention stripped).
        thread_context: Rendered prior thread messages, or "".
        catalog: Newest changelog versions, one per line, for version grounding.

    Returns:
        The route.json object as a dict.

    Raises:
        Exception: Any proxy/transport/parse error (caller falls back to bench).
    """
    user = f"Mention from a user:\n{text.strip()}\n"
    if thread_context:
        user += f"\nMessages earlier in this thread (oldest first):\n{thread_context}\n"
    user += f"\nChangelog entries that currently exist (newest first):\n{catalog}\n"
    user += "\nWrite route.json now."

    proxy = ProxyClient.from_env()
    req = RunAgentRequest(
        cell_id=f"bammy-route-{uuid.uuid4().hex[:10]}",
        model=INTENT_MODEL,
        max_turns=3,
        prompt=user,
        system_prompt=SYSTEM_PROMPT_ROUTE,
        post_file_patterns=["route.json"],
        invocation_timeout_secs=CLASSIFY_TIMEOUT_SECS,
    )
    result = await proxy.run_agent(req, timeout=CLASSIFY_TIMEOUT_SECS + 60)
    if result.status != "ok":
        raise RuntimeError(f"classifier run {result.status} (exit {result.exit_code})")
    raw = result.post_files.get("route.json")
    if raw:
        try:
            data = json.loads(raw)
            if isinstance(data, dict):
                return data
        except json.JSONDecodeError:
            pass
    scraped = extract_last_json_object(result.transcript or "")
    if isinstance(scraped, dict):
        return scraped
    raise RuntimeError("classifier did not produce a parseable route.json")


async def _thread_context(event: dict[str, Any]) -> str:
    """Fetch and render the thread messages preceding a mid-thread mention.

    Args:
        event: The Slack event (channel, ts, thread_ts).

    Returns:
        Rendered "name: text" lines (excluding the mention itself), or ""
        for a top-level mention or on any fetch failure.
    """
    thread_ts = event.get("thread_ts")
    ts = event.get("ts")
    if not thread_ts or thread_ts == ts:
        return ""
    try:
        messages = await slack_client.fetch_thread(
            _bot_token(), event.get("channel") or "", thread_ts, limit=THREAD_CONTEXT_LIMIT
        )
        messages = [m for m in messages if m.get("ts") != ts]  # drop the mention itself
        names: dict[str, str] = {}
        for uid in {m.get("user") for m in messages if m.get("user")}:
            info = await slack_client.users_info(_bot_token(), uid)
            if info:
                names[uid] = slack_client.display_name(info)
        return slack_client.render_thread(messages, names=names)
    except Exception:  # noqa: BLE001
        log.exception("bammy: thread context fetch failed (continuing without)")
        return ""


HELP_TEXT = (
    "Hi! Here's what I can do:\n"
    "- *Run a bench task*: `@bammy try building a recipe parser with baml` "
    "(directives: `[canary]` `[nightly]` `[baml=N]` `[coldstart]` `[skill arena]`)\n"
    "- *Edit the changelog*: `@bammy fix the title of the latest nightly`\n"
    "- *Catch the changelog up*: `@bammy add new changelog entries`\n"
    "- *T-shirt code*: `@bammy promo code please`\n"
    "- *Query PostHog*: `@bammy how many signups did we get this week?`\n"
    "- *Log feedback*: `@bammy feedback: baml fmt is slow on big files`"
)


async def route_mention(service: ServiceClient, event: dict[str, Any], text: str,
                        eid: Optional[str]) -> None:
    """Route one @bammy mention to the right handler (off the request path).

    Failures are logged, never raised — the Slack ACK is already gone.

    Args:
        service: Service client (the ingress module's live/faked instance).
        event: The Slack event object.
        text: The mention text with the leading bot mention stripped.
        eid: The Slack event id, for log correlation.
    """
    try:
        # 1. Directive fast-path: explicit bench opt-in, zero model calls.
        if bench.has_directives(text):
            ctx = await _thread_context(event)
            await bench.create_slack_task(service, event, text, eid, thread_context=ctx)
            return

        # 2. Thread context for the classifier and handlers.
        ctx = await _thread_context(event)

        # 3. Classify — fail open to bench (pre-bammy behavior) when the
        #    classifier is unavailable or errors.
        if not os.environ.get("CLAUDE_PROXY_URLS"):
            await bench.create_slack_task(service, event, text, eid, thread_context=ctx)
            return
        try:
            entries = await changelog_edit.list_entries(service, limit=60)
            catalog = "\n".join(
                f"- {e['version']}  ({e.get('channel', 'unknown')}, {e.get('date', '?')})"
                for e in entries
            ) or "(no entries yet)"
            intent = await _classify(text, ctx, catalog)
        except Exception:  # noqa: BLE001
            log.exception("bammy: classifier failed; falling back to bench (event_id=%s)", eid)
            await bench.create_slack_task(service, event, text, eid, thread_context=ctx)
            return

        route = intent.get("intent", "bench")
        log.info("bammy: event_id=%s route=%s intent=%s", eid, route, intent)
        channel = event.get("channel")
        thread = event.get("thread_ts") or event.get("ts")

        if intent.get("needs_clarification"):
            q = intent.get("clarification_question") or "Can you give me a bit more detail?"
            await slack_client.post_message(_bot_token(), channel, q, thread_ts=thread)
            return

        if route == "changelog_edit":
            await changelog_edit.handle(
                service, _bot_token(), event, intent,
                allowed_users=_changelog_allowlist(),
            )
        elif route == "changelog_sync":
            from bench_core.changelog_sync import sync_missing_entries

            try:
                enqueued = await sync_missing_entries(service)
            except Exception as e:  # noqa: BLE001 — GitHub flake: tell the user
                await slack_client.post_message(
                    _bot_token(), channel,
                    f"Couldn't check GitHub for new releases: {e}", thread_ts=thread,
                )
                return
            if enqueued:
                versions = ", ".join(f"`{e['version']}`" for e in enqueued[:8])
                more = f" (+{len(enqueued) - 8} more)" if len(enqueued) > 8 else ""
                await slack_client.post_message(
                    _bot_token(), channel,
                    f"Enqueued {len(enqueued)} new entr{'y' if len(enqueued) == 1 else 'ies'}: "
                    f"{versions}{more}. They'll publish as generation finishes.",
                    thread_ts=thread,
                )
            else:
                await slack_client.post_message(
                    _bot_token(), channel,
                    "Changelog is already up to date with GitHub.", thread_ts=thread,
                )
        elif route == "promo_claim":
            await promo.handle(service, _bot_token(), event, intent)
        elif route == "posthog_query":
            await posthog_query.handle(service, _bot_token(), event, intent)
        elif route == "feedback":
            message = (intent.get("feedback_text") or "").strip() or text
            if ctx:
                message = f"{message}\n\nSlack thread context:\n{ctx}"
            await feedback.create_feedback(
                service, message, origin="slack",
                slack={"slackChannel": channel, "slackThreadTs": thread,
                       "slackUser": event.get("user")},
            )
            await slack_client.post_message(
                _bot_token(), channel,
                "Logged — it'll be triaged into the issue board. Thanks!",
                thread_ts=thread,
            )
        elif route == "general":
            await slack_client.post_message(_bot_token(), channel, HELP_TEXT, thread_ts=thread)
        else:  # bench (and anything unexpected)
            prompt = (intent.get("bench_prompt") or "").strip() or text
            await bench.create_slack_task(service, event, prompt, eid, thread_context=ctx)
    except Exception:  # noqa: BLE001
        log.exception("bammy: routing failed for event_id=%s", eid)
