# Harness and agent runtime contracts

`Tools` and `Harness` describe different loop owners:

```text
BAML-owned loop: Request -> Tools.step -> app dispatch -> Tools.submit -> ...
external loop:   Request -> Harness session -> runtime-owned turns/tools -> AgentRun
```

Use `HarnessAgent` when a task only needs a final typed value. Use
`run_harness`, `resume_harness`, or `stream_harness` when the application needs
session lifecycle, stop reasons, turns, usage, reasoning, or tool events.

## State boundary

- `Conversation` is portable application data for UI, logging, export,
  compaction, and cross-provider handoff.
- `HarnessSessionToken` is provider-controlled continuation state. Store and
  return it unchanged. Do not reconstruct it from text or `Conversation`.
- `ModelBlock` and `AgentEvent` expose reasoning and provider metadata for
  observability without making those views the next request's source of truth.

This preserves Anthropic signatures, redacted thinking blocks, OpenAI reasoning
state, tool-call IDs, citations, and future provider-specific continuation data.

## Extension points

- Implement `Harness` for an external runtime. Its associated `HarnessSession`
  remains adapter-owned.
- Implement `HarnessControlPlane` for optional runtime commands.
- Add provider-specific `ModelBlock` or `AgentEvent` classes with out-of-body
  `implements`; the shared interfaces are method-only on purpose.
- Use `ToolMiddleware` for behavior-changing approval or rewriting.
- Use `AgentObserver` for UI/logging and `AgentRecorder` for persistence. These
  cannot silently change tool execution.
- Implement `Generate` independently when a runtime also has a genuine one-shot
  API. `Harness` does not imply `Generate` or `Tools`.
