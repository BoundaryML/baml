# Organizing `ai`

The flat `ai` namespace contains the contracts required for ordinary typed
model execution. Capability-specific machinery lives one namespace below it.
Provider-specific wire implementation stays in the provider's private
namespace.

## Layout

| Namespace | Contents |
| --- | --- |
| `ai` | `Task`, `ResponseWithMetadata`, `ResponseMetadata`, `Usage`, `Conversation`, `MessageHistory`, `Provider`, `AgentProvider`, `ModelStep`, `ResumableAgentProvider`, `ConversationImportProvider`, `StreamingProvider`, `Failure`, `Effects`, `retry`, `fallback`, `Done`, `BudgetReached`, `Handoff`, `Budget` |
| `ai.run` | `Agent`, `Stream`, `Background`, `Batch`, `Transcribe`, `VoiceAgent`, `Harness`, and other public runners |
| `ai.tools` | `Tool`, `ToolInput`, `ToolRegistry`, `ToolResult`, `ToolCall`, callbacks, and JSON-schema tool construction |
| `ai.realtime` | channels, live sessions/events, audio formats, and collection helpers |
| `ai.transcription` | transcription provider protocol and audio streams |
| `ai.sessions` | provider-owned session protocol |
| `ai.jobs` | background and batch protocols, jobs, batches, and options |
| `ai.observe` | Agent events, observers, recorders, and usage accounting |
| `ai.harness` | external harness protocol, sessions, options, and results |
| `ai.messages` | structural message parts and shared provider plumbing |
| `ai.testing` | deterministic fakes for providers and other capabilities |
| `ai.internal` | non-public execution and provider helper functions |

## Decisions

- `AgentProvider` stays flat because it is the normal provider execution
  capability.
- `ModelStep<T>` stays flat because it is the boundary between every Agent and
  normal provider.
- `Done`, `BudgetReached`, `Handoff`, and `Budget` stay flat because every
  explicit Agent caller uses them.
- `retry` and `fallback` stay flat because they are provider wrappers for the
  normal lifecycle.
- errors stay flat because their channel appears throughout public
  signatures.
- application tool values and hooks live in `ai.tools`.
- provider prompt-mode adapters do not live in `ai.tools`; OpenAI, Anthropic,
  and Google keep those adapters private because they retain provider-specific
  task render recipes and continuation state.
- Claude Code's private Agent adapter uses a CLI JSON-schema envelope for
  `T | ToolCalls`; it is not a prompt/SAP provider mode.

Provider namespaces expose configuration-sized APIs:

```text
openai.Responses / openai.responses(...)
anthropic.Messages / anthropic.messages(...)
google.vertex.Gemini / google.vertex.gemini(...)
google.ai.Gemini / google.ai.gemini(...)
claude_code.ClaudeCodeCli
```

Request envelopes, concrete conversation classes, schema transforms,
authentication helpers, prompt/SAP tool adapters, and Claude Code's
schema-envelope adapter are implementation details. They may change without
expanding the portable `ai` surface.
