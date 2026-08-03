# Observability

## The journal is the trace

There is no separate instrumentation step. Every model call, tool call,
and child session is already in the journal, in order, with token counts.
Observability is reading it.

```bash
baml session show trip-9421              # render a journal, human-readable
baml session tail trip-9421              # follow it live
baml session export trip-9421 --json     # the raw entries
```

## Two streams

Mirroring the runtime design, there are two surfaces:

- **The journal tail** — durable events, replayable from any `seq`.
  Complete and ordered. Build dashboards, audits, and UIs on this.
- **The log stream** — `log.info` output, token deltas, transient
  progress. Ephemeral, never journaled, never visible to the model.

If a question is "what happened?", the answer is in the journal. If it is
"what is it doing right now?", the log stream and the delta channel add
liveness on top.

## Usage and cost

`Usage` events record token counts per provider call. Cost accounting is
a fold:

```baml
function session_cost(j: Journal) -> float {
    j.entries.reduce((acc, en) -> {
        match (en.event) {
            let u: Usage => acc + cost_of(u),
            _ => acc,
        }
    }, 0.0)
}
```

The same fold powers `with_budget` middleware at runtime — the metric and
the enforcement read the same events, so they cannot disagree.

## The session tree is the trace tree

Child sessions link to parents by `child_id`. This maps directly onto
span trees: a session is a trace, a model turn or tool call is a span, a
child session is a child span. The OpenTelemetry exporter walks journals
and emits GenAI-convention spans:

```toml
# baml.toml
[observability.otel]
endpoint = "https://collector.internal:4317"
content = false        # spans without prompt/completion bodies
```

`content = false` exports timing, usage, tool names, and status only —
for environments where prompt content must not leave the process.

## Watching a live session from another process

Named instances and jobs are observable by ID from anywhere with access
to the journal store:

```python
from baml_sdk import b

for entry in b.session.CodeAgent.tail("issue-17307"):
    print(entry.seq, entry.event.kind)
```

This is the same endpoint UIs use (`../02_guides/13_serving.md`). An
operator attaching to a misbehaving agent sees exactly what the model
saw — the transcript is derivable from the same events, which is what
makes journal-based debugging trustworthy: there is no second, partial
record to reconcile.
