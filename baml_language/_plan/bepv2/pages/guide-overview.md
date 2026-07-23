# Guide — Overview

This guide builds one customer-support application with the proposed BEP-064
API. Each chapter introduces one group of related capabilities.

Start with the [running example](./guide-overview/00-running-example.md), then
follow the chapters in order or jump to the problem you need to solve.

## How to read the examples

Guide snippets use the proposed `ai.*` namespace and generated `.task(...)`
syntax. They are written for users of the final API. You do not need access to
the private conformance package.

Each page labels its implementation status:

- **Implemented:** the behavior exists in the reference implementation and is
  covered by tests. The snippet may use the proposed final `ai.*` spelling.
- **Partial:** the core path works, but the page names a missing capability.
- **Proposed:** design guidance that is not implemented yet.

Shared classes, providers, and application callbacks are introduced in the
running example. Each recipe includes the code needed to understand the new
API it introduces.

## Fast path

For the shortest introduction, read:

1. [Direct typed call](./guide-tasks-and-providers/01-direct-typed-call.md)
2. [Task and drivers](./guide-tasks-and-providers/02-task-and-drivers.md)
3. [One tool](./guide-tools-and-agents/01-one-tool.md)
4. [Agent loop](./guide-tools-and-agents/02-agent-loop.md)
5. [Retry safe calls](./guide-routing-and-reliability/01-retry-safe-calls.md)
6. [Application-owned history](./guide-conversations-and-state/01-application-owned-history.md)
7. [Observe an agent](./guide-observability-and-testing/02-observe-an-agent.md)
8. [Background jobs](./guide-production/01-background-jobs.md)

## Chapters

1. [Guide — Tasks and providers](./guide-tasks-and-providers.md)
2. [Guide — Tools and agents](./guide-tools-and-agents.md)
3. [Guide — Routing and reliability](./guide-routing-and-reliability.md)
4. [Guide — Conversations and state](./guide-conversations-and-state.md)
5. [Guide — Media and realtime](./guide-media-and-realtime.md)
6. [Guide — Observability and testing](./guide-observability-and-testing.md)
7. [Guide — Production](./guide-production.md)
8. [Guide — External harnesses](./guide-external-harnesses.md)

## Ownership shortcut

```text
task:        prompt, return type, default tools, and selected provider
driver:      lifecycle, loop, retry policy, and termination
provider:    wire protocol and exact transcript
application: handlers, UI, logs, and business data
resource:    one live provider or harness operation
```

The [specification](./specification.md) defines these boundaries in detail.
