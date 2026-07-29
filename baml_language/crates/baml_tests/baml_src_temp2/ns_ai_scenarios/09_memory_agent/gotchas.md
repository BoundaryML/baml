# Memory Agent port gotchas

This file records semantic differences between the original handwritten agent
loop and the BEPv4 Agent-framework port.

## A session has two kinds of state

The REPL retains both:

- an application transcript for display, memory search, and curator input;
- the exact provider-owned `ai.Conversation` used for the next model turn.

After the first turn, the scenario asks the conversation's
`ConversationAppendProvider` to append one fresh user message. The provider
updates its native continuation state locally, without making an HTTP request.
The next `Task.run(Agent.new(conversation = ...))` keeps OpenAI response IDs or
Google thought signatures instead of rebuilding a continuation from
`conversation.messages()`.

Exact append currently accepts fresh user text only. Tool results use
`AgentProvider.submit`, while system and assistant history remain
provider-owned. The capability rejects another provider instance's
conversation and any conversation with unresolved tool calls.

The selected provider is fixed once a session starts. The OpenAI and Google
entrypoints demonstrate the same Task recipe with different initial providers;
they do not switch an exact conversation between vendors. A deliberate vendor
switch uses `ConversationImportProvider` and provides only messages-fidelity
continuation.

## The Agent owns tool selection and dispatch

The original model returned a `Step` class containing an `action` string and
nullable argument fields, after which a handwritten loop dispatched the
action. The port exposes ordinary typed BAML functions as Agent tools. Native
provider tool calling selects one of those functions, and `ai.run.Agent`
validates, dispatches, correlates, and submits results.

Both selected providers set `parallel_tool_calls = false` as a request hint.
The scenario also constructs `Agent` with `max_tool_calls_per_step = 1`.
That runner limit is the provider-independent safety boundary: it rejects an
oversized call batch before tool events, approval callbacks, handlers, or
handoffs can cause an application effect.

## Thought text is not a portable provider feature

The original `Step.thought` was a required structured field. Native
tool-calling APIs do not expose a provider-neutral "thought" field, and hidden
reasoning should not be copied into application logs. Each coding tool
therefore has an optional `summary` argument: the model supplies one short,
user-visible explanation of the action, and the observer records it alongside
tool lifecycle events.

## Cancellation is cooperative and resumable

ESC + Enter calls the `CancelToken` passed to `Agent`; it does not cancel the
spawned future. `Agent` stops only at a committed loop boundary and returns
`Interrupted { conversation, steps_taken, reason }`.

If cancellation arrives while a model request or tool batch is running, the
runner finishes and submits the whole batch before returning the checkpoint.
It never returns a conversation with unresolved calls and never abandons half
of a parallel batch. A final typed value or handoff already produced by the
model wins the race. The supervisor awaits the real outcome, stores its exact
conversation, and the next user message continues from it without replaying
completed tools.

This is not hard cancellation: a slow HTTP request, shell command, or file
write is allowed to finish. `Future.cancel()` or wiring the same token into
`spawn` would terminate the future and discard the resumable `Interrupted`
value.

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

## Framework capabilities added for this port

The three framework follow-ups exposed by the first port are now implemented:

```baml
interface ConversationAppendProvider requires AgentProvider {
  function append_messages(
    self,
    conversation: Conversation,
    messages: Messages,
  ) -> Conversation
}

let runner = ai.run.Agent<string>.new(
  conversation = exact_continuation,
  cancel = cancel_token,
  max_tool_calls_per_step = 1,
)
```

OpenAI Responses, Anthropic Messages, Google AI, Google Vertex, Retry,
Fallback, and the test providers implement exact append. `Agent` emits
`RunInterruptedEvent`, then the generic `RunFinishedEvent` with outcome
`"interrupted"`, before returning the checkpoint.

One application-level limitation remains: timed terminal input is
POSIX-specific. A portable timed terminal-read primitive would remove the
scenario's `read -t` shell polling.

## Security boundary

Like the source agent, this scenario deliberately has the user's filesystem
and shell permissions. It is a framework example, not a sandbox. Run it from a
working directory where the selected model is allowed to inspect and modify
files.
