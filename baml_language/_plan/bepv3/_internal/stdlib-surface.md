# Proposed standard-library surface

This page gathers the conceptual API in one place. Public examples remain the
source of truth for user-facing ergonomics. Exact helper names may change
during implementation if the behavior and type relationships remain intact.

## Core

```baml
namespace ai {
  class Task<T, P extends Provider = Provider> {
    function run<R>(
      self,
      runner: R,
    ) -> R.Output throws R.Error
      where R: Runner<Task<T, P>>

    function with_provider<P2>(
      self,
      provider: P2,
    ) -> Task<T, P2>

    function with_tools(
      self,
      tools: ToolInput[],
    ) -> Task<T, P>

    function with_messages(
      self,
      messages: Messages,
    ) -> Task<T, P>

    function messages(self) -> Messages
    function tools(self) -> Tool[]
    function output_type(self) -> type
  }

  interface Runner<Input> {
    type Output
    type Error

    function run(
      self,
      input: Input,
    ) -> Self.Output throws Self.Error
  }

  class Response<T> {
    value: T,
    meta: Meta,
    conversation: Conversation?,
  }
}
```

## Agent outcomes

```baml
namespace ai {
  type AgentOutcome<T> =
    Done<T>
    | BudgetReached
    | Handoff

  class Done<T> {
    value: T,
    meta: Meta,
    conversation: Conversation,
  }

  class BudgetReached {
    reason: BudgetReason,
    conversation: Conversation,
    steps_taken: int,
    meta: Meta,
  }

  class Handoff {
    to: string,
    reason: string?,
    conversation: Conversation,
    meta: Meta,
  }

  error AgentIncomplete {
    outcome: BudgetReached | Handoff,
  }
}
```

## Standard runners

```baml
namespace ai.run {
  class Completion
  class CompletionWithMeta
  class Generation
  class GenerationWithMeta
  class Stream
  class Agent
  class Background
  class Batch<T>
  class VoiceAgent
  class Harness<T>
  class Retry<Inner>
  class Fallback<Inner>
  class Transcribe
  class TranscribeWithMeta
}
```

The Agent runner keeps its configuration directly:

```baml
let agent = ai.run.Agent.new(
  tools = null,
  conversation = null,
  max_steps = 12,
  max_cost_usd = null,
  hooks = null,
  observers = [],
  tool_registry = null,
)
```

For `tools`:

| Value | Meaning |
| --- | --- |
| `null` | Inherit the LLM function's declared application tools |
| `[]` | Offer no application tools |
| non-empty list | Replace the declared application tools |

If `tool_registry` is non-null, it is the authoritative roster and `tools`
must be `null`.

## Provider capabilities

```baml
namespace ai {
  interface Provider
  interface CompletionProvider requires Provider
  interface GenerationProvider requires Provider
  interface StreamingProvider requires Provider
  interface ToolCallingProvider requires Provider
  interface BackgroundProvider requires Provider
  interface BatchProvider requires Provider
  interface RealtimeProvider requires Provider
  interface ConversationImportProvider requires Provider
}
```

Providers may implement any valid subset. Runners express their requirements
through generic constraints.

## State

```baml
namespace ai {
  interface MessagePart
  interface Message
  class ChatMessage
  interface Messages
  class MessageHistory
  interface Conversation<P extends Provider = Provider>
  class ConversationToken
  class ConversationImport<P>
  enum ConversationFidelity
}
```

## Tools

```baml
namespace ai {
  type ToolInput = baml.AnyFunction | Tool

  class Tool {
    name: string,
    description: string,
    input_schema: json,
    handler: baml.AnyFunction,
    handoff: bool,
  }

  class ToolRegistry
  class ToolCall
  class ToolResult
  class InvalidToolArguments
  interface AgentHooks
  interface AgentObserver
}
```

`ai.tool(function)` derives a `Tool` from a function's signature and
documentation. Plain function values are normalized the same way. `Tool` is
metadata plus an executable function, not a nominal interface that application
functions must implement.

## Resources

```baml
namespace ai {
  interface Stream<TPartial, T>
  interface Job<T>
  interface Batch<T>
  class BatchItem<T>
  class BatchQueue
  interface Cache
  interface LiveSession
  interface HarnessSession
  interface Session
  interface McpConnection
}
```

Raw realtime is opened through a function:

```baml
ai.open_live(task, channel) -> LiveSession
ai.open_session(provider) -> Session
ai.create_cache(provider, messages, ttl) -> Cache
```

A raw bidirectional session is opened directly because it is not a
task-in/result-out lifecycle.
