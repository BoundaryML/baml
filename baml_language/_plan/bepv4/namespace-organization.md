# Organizing `ai`: core stays flat, capabilities get sub-namespaces

The flat `ai` namespace currently exposes everything from `Task<T>` to fake
realtime sessions. Comments on the BEP (Sam: "IDK how to navigate this with
`baml describe`"; 2kai: "differentiate core vs out-of-the-box implementations
on a namespace level") both point the same way, and `ns_ai/ns_run` → `ai.run`
already set the precedent: a sub-namespace is a directory named `ns_*` inside
`ns_ai/`.

**Criterion:** a name stays in flat `ai` iff a first afternoon with BAML
touches it. Capability-specific machinery moves to `ai.<capability>`;
test doubles move to `ai.testing`; things only provider adapters touch live
with their capability.

## Proposed layout

| Namespace | Contents (moved from flat `ai`) |
| --- | --- |
| `ai` (core, stays flat) | `Task`, `Response`, `ResponseWithMetadata`, `Meta`, `Usage`, `Conversation`, `MessageHistory`, `Provider`, `CompletionProvider`, `GenerationProvider`, `StreamingProvider`, `Failure`, `Effects`, default errors, `retry()`, `fallback()`, `Done`, `BudgetReached`, `Handoff`, `Budget` |
| `ai.run` | (already exists) all runners |
| `ai.tools` | `Tool`, `ToolInput`, `ToolRegistry`, `ToolResult`, `ToolCall`, callbacks, capability negotiation, prompt-fallback rendering |
| `ai.realtime` | `Channel`, `LiveSession`, `LiveEvent`, audio formats, collect helpers |
| `ai.transcription` | transcription protocol + audio stream types |
| `ai.sessions` | provider-owned session protocol |
| `ai.jobs` | background + batch protocols (`Job`, `Batch`, options) |
| `ai.observe` | observability events, observers, usage accounting internals |
| `ai.harness` | harness protocol and models |
| `ai.messages` | message parts, prompt adapters (raw internals; `Conversation`/`MessageHistory` stay core) |
| `ai.testing` | every fake: `FakeProvider`, fake tools, fake realtime/sessions/transcription/background/batch |
| `ai.internal` | hidden plumbing (`_run_tool_calls`, `_emit_agent_event`, `_add_usage`, `_may_replay`, `classify_http`): not public surface, may change shape without notice, never appears in examples |
| `google` (not `ai`) | named-cache protocol + `CreateCache` (done — Gemini-only concept) |

Judgment calls, flagged:

- **Outcomes and `Budget` stay core.** Every agent caller matches on
  `Done`/`BudgetReached`/`Handoff`; pushing them to `ai.agent` would put the
  most-typed names behind a prefix.
- **`retry`/`fallback` and `ReplayPolicy`/`ReplayKind` stay core** — the
  replay contract is part of `CompletionProvider`'s signature; only the
  `_may_replay` judgment moved to `ai.internal`.
- **Errors stay core.** The channel appears in every signature; making it
  `ai.failures.Failure` would tax every throws clause.

## Migration mechanics

Directory renames inside `ns_ai/` (`tools/` → `ns_tools/`, `resources/realtime/`
→ `ns_realtime/`, ...) plus a requalification sweep (same shape as the cache
move: bare names in moved files gain `root.ai.` prefixes; external references
gain the sub-namespace segment). Scenario files and BEP pages update in the
same pass; the grounding audit re-runs after.

Estimated blast radius: most of the 179 corpus files touch `ai.tools` or a
resource protocol at least once; the sweep is mechanical but should land as
its own commit.
