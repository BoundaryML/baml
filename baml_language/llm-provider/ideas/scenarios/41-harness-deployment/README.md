# 41 — Embedding & deployment of a harness

This scenario stress-tests whether the Provider/capability model can express *embedding and deploying an agent*: how a host drives a harness over three divergent transports (subprocess+JSONL, JSON-RPC subprocess, in-process generator) all behind `Provider.call` / `Realtime.run`; how an agent becomes a deployable unit with declarative webhook/cron triggers and a durable `<id>` instance under single-writer semantics; the author-vs-consume split (runtime SDK classes vs an HTTP client to a deployed agent); and the clash between provider config as a declarative `client` block versus runtime `registerProvider`. The load-bearing addition is a `Drivable` capability (`type Drive`, `HostMsg`/`AgentMsg`) that hides the JSONL-vs-RPC-vs-generator divergence inside `pump`, plus `Durable`, `Trigger`/`DeployedAgent`, and a host provider registry.

Background: background/06-harnesses.md → ## 6. ◆ Embedding & deployment
