# Agent Activity: ideas for later

A backlog of directions for the Agent Activity UI (`baml agent watch`). None of
these are built yet.

## Dependency trail for an edit

When an edit changes `funcD`, show why it matters by walking up to the code that
depends on it, laid out like a call stack:

```
funcA(order: Order) -> Receipt             ← signature only
  └─ funcB(items: Item[]) -> Total          ← signature only
       └─ funcC(item: Item) -> Money        ← signature only
            └─ funcD                         ← the edit itself, as a diff
```

- Each caller is a box that shows only its signature. The boxes are linked by
  lines and indented one level per hop, like a call stack.
- The edit on `funcD` sits at the bottom as the full diff.
- Show indirect dependents too, not only direct callers: `funcA` depends on
  `funcD` through `funcB` and `funcC`.
- To build this we need a reference index (who calls or uses what) that goes
  beyond the per-file outline the UI extracts today. For BAML files the
  compiler can supply real call and type references. For other languages it
  would need tree-sitter or an LSP.

## Pacing

- When edits are large or arrive faster than they can be shown, animate them
  more slowly and put the rest in a queue that plays while the agent is
  thinking. The player speeds up in proportion to the backlog, and past a
  limit it applies edits without animating them.

## BAML graph

- Render the playground's control-flow graph for BAML functions and update it
  live as edits land, so a changed `function` shows its new shape.
- When an edit lands, the diff card could fly into the graph and turn into (or
  merge with) the node it changed.

## Runs with real execution traces

- Have `baml run` and `baml test` report to a running `baml agent watch`
  (`POST /api/agent-activity/runs`, already served) with every function that
  executed, each LLM call's client, model, and tokens, and the return value.
  Today a run read from the transcript knows only its target function and
  output. With the full trace, Flow could light each step as it executes.
- Map run frames to graph nodes by call site (the playground notes this
  mapping has to be rebuilt from profiles-v1 call sites), not by function name.

## Sessions

- `baml agent watch --record session.json`: the recorded-session format is
  already the wire protocol plus a time offset, so a live session can be saved
  and replayed exactly like the built-in recordings.
- Several agents at once: give each transcript session its own color and
  caption lane.
- Codex transcripts (`~/.codex/sessions/**/rollout-*.jsonl`) for captions and
  command runs, alongside Claude Code's.

## Knowing more than the file system says

- Link edits to the agent's transcript (Claude Code / Codex session files or
  hooks). That would show the reads, searches, and commands between bursts,
  and the reason the agent gave for each edit.
- Mark the edits that came from shell commands (`sed -i`, heredocs, Python
  scripts) differently from editor tool calls. They are nearly half of all
  file changes in recorded sessions.
- Show failing builds or tests between bursts, when a hook can report them.

## The agent's scratch directory

Agents do part of their work outside the project, such as scripts, probes,
and generated test data in a temporary directory. For example, Claude Code
gives each session a scratchpad under
`/private/tmp/claude-<uid>/<encoded-cwd>/<session-id>/scratchpad`.

- When a transcript shows that a session is working in the watched directory
  (its `cwd` is under the root, which the transcript reader already checks),
  also watch that session's scratch directory.
- Show only scratch files that relate to this session's work, such as files
  the session's own tool calls wrote or that its commands in the project
  refer to. Other agents' scratch files and unrelated leftovers stay hidden.
- Draw scratch files in their own frame, apart from the project's files, so
  it is clear they are not part of the change.
