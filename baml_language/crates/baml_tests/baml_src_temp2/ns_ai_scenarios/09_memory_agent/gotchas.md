# Memory Agent port gotchas

This file records semantic differences between the original handwritten agent
loop and the BEPv4 Agent-framework port.

## Provider conversations do not yet accept a new application message

`ai.Conversation` can resume an interrupted provider/tool turn, but the
current provider protocol has no public operation for appending a fresh user
message to a completed conversation. The interactive session therefore keeps
an application-owned transcript and creates a new `Task<string>` for each
user turn. That preserves behavior and makes OpenAI/Google switching trivial,
but it resends the retained transcript instead of continuing an opaque
provider conversation. Rebuilding from `conversation.messages()` is not an
equivalent workaround: a message-only projection can lose provider response
IDs, encrypted reasoning, Google thought signatures, and provider data parts
used for native tool calls.

## The Agent owns tool selection and dispatch

The original model returned a `Step` class containing an `action` string and
nullable argument fields, after which a handwritten loop dispatched the
action. The port exposes ordinary typed BAML functions as Agent tools. Native
provider tool calling selects one of those functions, and `ai.run.Agent`
validates, dispatches, correlates, and submits results. Both requested
providers set `parallel_tool_calls = false`, which enforces or requests one
call per step for these adapters. An arbitrary custom provider can still
return a batch because `Agent` does not currently enforce a provider-neutral
per-step call limit.

## Thought text is not a portable provider feature

The original `Step.thought` was a required structured field. Native
tool-calling APIs do not expose a provider-neutral "thought" field, and hidden
reasoning should not be copied into application logs. Each coding tool
therefore has an optional `summary` argument: the model supplies one short,
user-visible explanation of the action, and the observer records it alongside
tool lifecycle events.

## Cancellation returns application state, not provider continuation state

ESC + Enter cancels the spawned `Task.run` future, including an in-flight HTTP
request. As in the original implementation, the transcript records an
interruption and the next user turn starts from application-owned history.
There is no portable provider continuation to resume after cancellation.
Cancellation also cannot roll back a shell command or file write that already
started, so a cancelled effect may be partially committed.

## Terminal polling is POSIX-specific

The original non-blocking `read -t` polling and `/dev/tty` output are also used
here. The interactive entrypoints are intended for macOS/Linux terminals.
The deterministic tool, memory, queue, and task-construction tests do not
depend on an interactive TTY.

## The same override is used for curation

The original hard-coded a separate Anthropic Haiku client for memory
curation. To make each public entrypoint require only one vendor credential,
the port sends both the coding task and the curator task through the selected
OpenAI or Google provider. They remain separate Tasks and context windows.

## No new Provider or Runner is needed

This is an ordinary tool-using model lifecycle. OpenAI and Google own wire
formatting and one `begin` / `step` / `submit` conversation at a time. The
existing `ai.run.Agent` runner owns the bounded loop, application tool
execution, correlation, and events. The REPL, queue, transcript, cancellation
supervisor, and durable memory are application state above `Task.run`.

A custom Provider would incorrectly mix terminal and filesystem effects into a
model adapter. A custom Runner would duplicate the Agent loop and recreate the
completion-provider/runner recursion problem this API split is intended to
avoid.

## Recommended framework follow-ups

The scenario does not need these changes to match the source behavior, but a
production framework should consider:

1. A provider capability for appending fresh messages to an exact
   `Conversation`, preserving provider response IDs, cache state, encrypted
   reasoning, media, and thought signatures.
2. Cooperative Agent cancellation with an `Interrupted` outcome containing
   the last committed conversation and step count, plus a matching lifecycle
   event.
3. A runner-level `max_tool_calls_per_step` policy for providers that cannot
   enforce serial tool calls. The OpenAI and Google adapters used here both set
   `parallel_tool_calls = false`, so this scenario does not need it.
4. A portable timed terminal-read primitive. Type-ahead currently uses the
   same POSIX shell polling technique as the source agent.

A minimal continuation capability would look roughly like:

```baml
interface ConversationAppendProvider requires AgentProvider {
  function append_messages<T>(
    self,
    conversation: Conversation,
    messages: Messages,
  ) -> Conversation
}
```

The provider, rather than the application, must implement this so the returned
conversation can retain exact provider state.

## Security boundary

Like the source agent, this scenario deliberately has the user's filesystem
and shell permissions. It is a framework example, not a sandbox. Run it from a
working directory where the selected model is allowed to inspect and modify
files.
