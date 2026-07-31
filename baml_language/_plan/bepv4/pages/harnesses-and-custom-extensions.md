# Harnesses and custom extensions

An external coding or research harness owns a larger lifecycle than an
`AgentProvider`: workspace permissions, its own tools, session control,
steering, interruption, and event transport. Use `ai.run.Harness` for that
case.

Implement `ai.Runner<Input>` when a reusable task lifecycle is not already
present in `ai.run`.

## Run a task through a harness

```baml
function InvestigateRepository(ticket: SupportTicket) -> Resolution {
  provider: fast_model()
  prompt: `
    Investigate this ticket in the repository.
    Ticket: ${ticket.id}
    ${ctx.output_format}
  `
}

let run = InvestigateRepository@task(sample_ticket()).run(
  runner = ai.run.Harness<Resolution>.new(
    fake_model_harness(),
    cwd = "/workspace",
    permission_mode = "read-only",
    sandbox = "workspace",
    attributes = { "team": "support" },
  ),
);

let value: Resolution = run.value
```

`ai.harness.HarnessRun<T>` retains the value, normalized events, portable
history, and any resume information.

The harness adapter must enforce requested permission and sandbox boundaries.
These are runtime controls, not prompt suggestions.

Runnable examples:

```console
baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.call_a_coding_harness

baml run --from crates/baml_tests/baml_src_temp2 \
  ai_scenarios.name_a_harness
```

## Observe harness events

```baml
let events: string[] = [];

let run = InvestigateRepository@task(sample_ticket()).run(
  runner = ai.run.Harness<Resolution>.new(
    fake_model_harness(),
    cwd = "/workspace",
    permission_mode = "read-only",
    on_event = (event: ai.observe.AgentEvent) -> null {
      events.push(event.kind());
      null
    },
  ),
)
```

The callback observes events while work is active. The same events are
retained in `run.events`. It does not change permissions, rewrite events, or
steer the session.

## Control a session directly

Use the explicit `ai.harness.HarnessSession` API for bidirectional control:

```baml
let harness = fake_model_harness();

let session = harness.open(
  ai.harness.HarnessOptions {
    cwd: "/workspace",
    permission_mode: "read-only",
    sandbox: "workspace",
    attributes: { "team": "support" },
  },
);

harness.steer(session, "Focus on billing code.");

let run = harness.run<Resolution>(
  session,
  InvestigateRepository@task(sample_ticket()),
  null,
);

let token = harness.save_session(session)
```

`interrupt`, `save_session`, and `restore_session` belong to this harness
state machine. The normal Harness runner opens and owns a session when the
application does not need direct control.

## Claude Code has two explicit roles

`claude_code.ClaudeCodeCli` adapts the local Claude Code CLI. It can be used
through the external harness protocol and can also implement `AgentProvider`
for typed task execution.

```baml
let claude = claude_code.ClaudeCodeCli {
  executable: "claude",
  model: "haiku",
  cwd: "/workspace",
  timeout_ms: 60000,
  max_turns: 3,
  max_budget_usd: "0.10",
  permission_mode: "dontAsk",
  tools: ["WebFetch"],
  allowed_tools: ["WebFetch"],
  safe_mode: true,
  persist_session: false,
  harness_sessions: [],
}
```

Provider adapters do not hand-add identity fields: instance identity is
built into the runtime, and `ai.same_provider_instance(a, b)` is the public
check — true for the same provider instance, or one reachable through the
other's `delegate()` chain. Conversation-ownership guards use exactly this
rule.

These roles do not recurse into one another:

- `ai.run.Harness` uses the CLI's harness session protocol;
- `ai.run.Agent` uses its `begin`/`step`/`submit` protocol;
- a Claude Code `step` may invoke Claude Code's built-in tools internally;
- a BAML application tool returned as `ToolCalls` is executed only by the
  outer BAML Agent.

The CLI-backed provider's JSON-schema envelope and conversation state are
private. Applications configure `ClaudeCodeCli`; they do not construct its
`T | ToolCalls` adapter.

## Custom runners

A custom runner has a precise input, output, and error channel:

```baml
class NegotiatedRun {
  provider: string,
  mode: string,
  output: string,
}

class CapabilityNegotiationRunner {
  function new() -> CapabilityNegotiationRunner throws never {
    CapabilityNegotiationRunner {}
  }

  implements ai.Runner<ai.Task<Resolution>> {
    type Output = NegotiatedRun
    type Error = ai.Failure
      | baml.errors.UnknownError
      | baml.errors.Unsupported

    function run(
      self,
      task: ai.Task<Resolution>,
    ) -> NegotiatedRun throws ai.Failure
        | baml.errors.UnknownError
        | baml.errors.Unsupported {
      match (task.provider) {
        let realtime: ai.realtime.RealtimeProvider => {
          let session = realtime.open_live<Resolution>(
            task,
            ai.testing.RecordingChannel { frames: [] },
          );
          NegotiatedRun {
            provider: realtime.name(),
            mode: "realtime",
            output: ai.realtime.collect_live_text(session),
          }
        },
        let stream: ai.StreamingProvider => {
          let value = stream
            .stream<Resolution$stream, Resolution>(task)
            .final();
          NegotiatedRun {
            provider: stream.name(),
            mode: "stream",
            output: value.reply,
          }
        },
        let provider: ai.AgentProvider => {
          let outcome = task
            .with_provider(provider)
            .run(runner = ai.run.Agent<Resolution>.new());
          match (outcome) {
            let done: ai.Done<Resolution> => NegotiatedRun {
              provider: provider.name(),
              mode: "agent",
              output: done.value.reply,
            },
            let stopped: ai.Stopped => throw baml.errors.Unsupported {
              message: "negotiated run stopped: " + stopped.reason,
            },
            let handoff: ai.Handoff => throw baml.errors.Unsupported {
              message: "negotiated run handed off to " + handoff.call.name,
            },
            let interrupted: ai.Interrupted => throw baml.errors.Unsupported {
              message: "negotiated run interrupted: " + interrupted.reason,
            },
            let failed: ai.Failed => match (failed.cause) {
              let failure: ai.Failure => throw failure,
              let unknown: baml.errors.UnknownError => throw unknown,
            },
          }
        },
        _ => throw baml.errors.Unsupported {
          message: "task provider has no supported lifecycle",
        },
      }
    }
  }
}
```

This runner's goal is capability negotiation: after receiving an erased
`ai.Provider`, it selects the strongest supported interaction and reports
which mode it used. It is not an alternate model loop. The Agent arm delegates
normal execution back to `ai.run.Agent`.

Keep the runner in `custom_runner.baml` beside the example that owns it. Keep
its helper classes and functions in `utilities.baml`.

## Writing a provider adapter

The full walkthrough is [Implement a provider](implement-a-provider.md).
The load-bearing facts for adapter authors:

- `render_shorthand()` returns `"vendor/model"` — any two non-empty
  segments joined by `/`; a malformed value raises `ai.InvalidRequest` at
  first render, and agent runs validate it up front.
- Thin wrappers (retry-like middleware, instrumentation) implement
  `delegate()` to return their inner provider; conversation-ownership
  checks walk that chain, so delegating wrappers are legal at any nesting
  depth. `ai.same_provider_instance(a, b)` is the public form of the check.
- `Conversation.output_type_fingerprint()` may return null — the guard is
  skipped, like `pending_calls`' null convention. Opt in to the wrong-task
  protection by reporting `ai.output_fingerprint<T>()` from conversations
  your `begin<T>` creates.
- The public helper kit covers what adapters used to hand-write:
  `ai.Usage.zero()` and `usage.add(next)` for accumulation,
  `ai.tools.check_calls(calls, provider_name)` and
  `ai.tools.check_results(pending, results, provider_name)` for the batch
  and correlation rules the stock Agent enforces,
  `ai.classify_http(provider, status_code, body)` for the shared HTTP
  status → failure table, and `task.recipe()` for the prompt-render recipe
  an adapter renders with its own output-format conventions.

## When to add what

| Need | Add |
| --- | --- |
| Normal provider model turns | `ai.AgentProvider` |
| Partial output | `ai.StreamingProvider` |
| Background or batch submission | Capability in `ai.jobs` |
| Realtime channel/session | Capability in `ai.realtime` |
| Transcription | Capability in `ai.transcription` |
| External tool-owning sandbox | `ai.harness.Harness` |
| Reusable application-visible lifecycle | `ai.Runner<Input>` |
| Only a label or application helper | Ordinary BAML function/class |

A provider adapter changes authentication, wire rendering, response parsing,
or provider-owned continuation state. A runner changes how a `Task` proceeds
or the type it returns.

## Provider capability tree

```text
ai.Provider
├── ai.AgentProvider
│   ├── ai.ResumableAgentProvider
│   ├── ai.ConversationImportProvider
│   └── ai.ConversationAppendProvider
├── ai.StreamingProvider
├── ai.jobs.BackgroundProvider
├── ai.jobs.BatchProvider
├── ai.transcription.TranscriptionProvider
└── ai.realtime.RealtimeProvider

ai.harness.Harness  // separate external-agent lifecycle
```

An adapter implements only the capabilities it can execute honestly. A custom
runner narrows `task.provider` to the smallest capability it needs and rejects
an unsupported pairing before making a request.
