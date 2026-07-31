# Implement a provider adapter

A provider adapter owns authentication, request rendering, response parsing,
and exact continuation state — nothing else. It never executes application
tools and never owns a model/tool loop. This page walks a complete minimal
adapter and names every obligation; everything it uses is public API.

## The minimal adapter

Two classes and three methods. `AcmeProvider` speaks a fictional HTTP API.

```baml
class AcmeConversation {
  owner: AcmeProvider,
  history: ai.MessageHistory,

  implements ai.Conversation {
    function provider(self) -> ai.Provider throws never { self.owner }
    function messages(self) -> ai.Messages throws never { self.history }
    // Optional protections, covered below:
    //   output_type_fingerprint() — defaults to null (guard skipped)
    //   pending_calls()           — defaults to null (session guards skipped)
  }
}

class AcmeProvider {
  api_key: string,
  model: string,

  implements ai.Provider {
    function name(self) -> string throws never { "acme" }
    // Grammar: "vendor/model" — any two non-empty segments. Selects
    // prompt-rendering defaults; does not need to be a real vendor.
    function render_shorthand(self) -> string throws never {
      "acme/" + self.model
    }
  }

  implements ai.AgentProvider {
    function begin<T>(self, task: ai.Task<T>) -> ai.Conversation
        throws ai.Failure | baml.errors.UnknownError {
      // Render the task's prompt into your opening state. No model request.
      AcmeConversation { owner: self, history: task.messages().to_history() }
    }

    function step<T>(
      self,
      conversation: ai.Conversation,
      tools: ai.tools.Tool[],
    ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
      let state = self._own(conversation);
      //# One model request. On a non-2xx response, classify with the
      //# SHARED table so retry/fallback treat your adapter like the
      //# built-ins (429 → RateLimited, 5xx/408 → NetworkFailure, other
      //# 4xx → InvalidRequest):
      //#   throw ai.classify_http("acme", response.status, response.text())
      //# Parse the body into either the typed value or tool calls.
      let value = ...; // parse<T>
      ai.ModelStep<T> {
        outcome: value,
        metadata: ai.ResponseMetadata {
          provider: "acme", model: self.model, request_id: ...,
          finish_reason: "stop", usage: ..., attributes: {}, raw: ...,
        },
        // Optional display channels, when your API exposes them:
        assistant_text: null,
        reasoning_text: null,
      }
    }

    function submit(
      self,
      conversation: ai.Conversation,
      results: ai.tools.ToolResult[],
    ) -> ai.Conversation throws ai.Failure | baml.errors.UnknownError {
      let state = self._own(conversation);
      //# Validate correlation with the shared rule — every pending call
      //# answered exactly once:
      //#   ai.tools.check_results(pending, results, "acme")
      //# Record the results in your wire history. No model request.
      state
    }
  }

  // Reject another instance's conversation with the same rule the stdlib
  // applies (instance identity, walking delegate() chains):
  function _own(self, conversation: ai.Conversation) -> AcmeConversation throws ai.Failure {
    match (conversation) {
      let state: AcmeConversation => {
        if (!ai.same_provider_instance(state.owner, self)) {
          throw ai.InvalidRequest {
            provider: "acme", status_code: null,
            detail: "conversation owned by another acme instance",
          };
        }
        state
      },
      _ => throw ai.InvalidRequest {
        provider: "acme", status_code: null,
        detail: "another provider's conversation",
      },
    }
  }
}
```

That is the whole required surface. Point a task at it:

```baml
let outcome = ResolveTicket@task(sample_ticket())
  .with_provider(AcmeProvider { api_key: ..., model: "acme-1" })
  .run(runner = ai.run.Agent<Resolution>.new());
```

## The obligations, in order of importance

1. **Commit only after wire success.** `step`, `submit`, and (if you
   implement it) `append_messages` mutate conversation state only AFTER the
   request succeeded. A failing call leaves the conversation exactly as it
   was — this is what makes step-level `ai.retry` replay-safe and lets the
   Agent guarantee that a throw means "nothing changed."
2. **Classify errors with `ai.classify_http`.** The shared table is what
   retry defaults and `retry_if` predicates match on. A hand-rolled mapping
   silently changes reliability behavior.
3. **Correlate with `ai.tools.check_results`.** Every pending call ID gets
   exactly one result, nothing extra. One rule, one message, everywhere.
4. **`messages()` is the whole transcript.** Include committed assistant
   output and tool results, not just user inputs — `session.export()`,
   `turns()`, and cross-provider moves are only as complete as this
   projection.
5. **Ownership.** Reject conversations owned by another instance via
   `ai.same_provider_instance`. If you write a thin wrapper that forwards
   `begin`/`step`/`submit` to an inner provider, declare it:
   `function delegate(self) -> ai.Provider? throws never { self.inner }` —
   ownership checks walk the chain, so wrappers are legal at any depth.

## Optional protections (opt in, never required)

- **Output-type guard**: have your conversation return
  `ai.output_fingerprint<T>()` (capture it in `begin`) from
  `output_type_fingerprint()`. The runtime then refuses to resume your
  conversation under a task with a different output type. The default null
  skips the guard.
- **Session guards**: report `pending_calls()` truthfully and sessions gain
  the typestate errors ("this is a handoff — call submit_tool_results")
  plus `phase()` accuracy. Null skips those guards.
- **Display channels**: populate `ModelStep.assistant_text` /
  `reasoning_text` when your API exposes visible text or reasoning
  summaries; the Agent turns them into `AssistantTextEvent` /
  `ReasoningEvent` for observers.

## Optional capabilities

Implement only what you can execute honestly:

| Capability | Adds |
| --- | --- |
| `ai.ConversationAppendProvider` | local mid-conversation appends (user text/media, steering notes). Mutates in place, returns null, validates with `ai.internal`-free rules: batch-validate BEFORE mutating |
| `ai.ResumableAgentProvider` | `save_conversation`/`restore_conversation` — sessions gain `fork()` and `save()`/`restore` |
| `ai.ConversationImportProvider` | `import_messages` — sessions gain `from(task, messages)` and `move_to` INTO your provider |

For custom runners rather than providers: `ai.Usage.zero()` /
`usage.add(next)` cover the accumulation arithmetic, and
`ai.tools.check_calls` applies the stock Agent's batch validation before any
application effect.
