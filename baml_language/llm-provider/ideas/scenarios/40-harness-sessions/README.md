# 40 — Built-in tools + on-disk sessions

A coding harness ships an opinionated toolset (Read/Edit/Bash/Grep) you allow-or-deny rather than define, and persists its conversation as JSONL on disk keyed by `cwd` — so you can continue, resume, fork, list, and tag sessions without re-running the agent. This scenario shows the proposed model expressing that by making the harness a `Provider` whose `SessionStore` (the 4-method protocol reused from scenario 17) is a real filesystem object, and by adding one negotiated capability, `SessionCatalog`, for the queryable helpers and the continue/resume/fork verbs. It builds directly on scenario 37 (the `ClaudeCode` subprocess harness + control plane) and stresses three things the model surfaces but cannot fully tame: the `cwd`-keyed-storage footgun, the lack of a compile-time resume guarantee, and a transcript store that is authoritative *outside* the BAML process.

Background: background/06-harnesses.md → ## 5. ◆ Built-in tools + on-disk sessions
