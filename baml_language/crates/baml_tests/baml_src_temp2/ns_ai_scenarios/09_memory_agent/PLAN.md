# Memory Agent port plan

Source: `BoundaryML/baml-demos/aie/baml_src/ns_agent` on `main`, including
`agent.baml`, `clients.baml`, and `tests.baml`. The agent's required
`ns_memory/memory.baml` and `ns_memory/curator.baml` behavior is included in
the parity target. `bad.baml` is excluded because it contains only commented
scratch code and is not part of the running agent.

## Entry points and provider selection

- [x] Add a zero-argument interactive OpenAI entry point:
  `ai_scenarios.memory_agent`.
- [x] Add a zero-argument interactive Google AI override:
  `ai_scenarios.memory_agent_google`.
- [x] Add zero-argument one-shot OpenAI and Google entry points for scripting.
- [x] Route both the main coding turn and the memory curator through the
  selected provider.
- [x] Execute model work through `Task.run(runner = ai.run.Agent.new(...))`.

## Coding-agent loop

- [x] Keep application-owned transcript state across user turns.
- [x] Give the model exactly one tool action per provider step.
- [x] Support a bounded 30-step Agent run.
- [x] Preserve the original 4096-token main-agent output cap.
- [x] Return a final typed answer.
- [x] Show model/tool progress through Agent events rather than maintaining a
  second handwritten model/tool loop.
- [x] Preserve a readable transcript containing user messages, tool calls,
  tool results, and assistant answers.

## Coding tools

- [x] Read files with line numbers.
- [x] Create or overwrite files.
- [x] Replace the first exact text occurrence.
- [x] List directories with directory markers.
- [x] Run shell commands and report exit code, stdout, and stderr.
- [x] Return recoverable tool-error text instead of crashing the session.
- [x] Preserve human-readable tool-call labels and clipped terminal output.

## Interactive session

- [x] Provide a terminal REPL with a banner and prompt.
- [x] Preserve ANSI styling and honor `NO_COLOR`.
- [x] Support `exit`, `quit`, and `/exit`.
- [x] Support `/queue`, `/q`, `/queue <task>`, `/q <task>`, and `/clear`.
- [x] Run queued tasks in order while threading transcript state.
- [x] Accept type-ahead while a turn runs and append it to the queue.
- [x] Cancel an in-flight turn with ESC followed by Enter.
- [x] Keep queued work after direct-turn cancellation and stop a queued batch
  after cancellation.
- [x] Provide `/memory`, `/mem`, and `/forget <name>`.

## Durable memory

- [x] Store each memory as a human-readable Markdown file.
- [x] Support configurable memory directory and an environment toggle.
- [x] Slugify stable memory names.
- [x] Normalize keywords.
- [x] Parse both frontmatter memories and hand-written headerless notes.
- [x] Search using weighted name/keyword/body token matches.
- [x] Filter stopwords and tolerate simple singular/plural prefixes.
- [x] Rank matches, cap recall, and avoid duplicate injection.
- [x] Inject recalled memory before the current user message.
- [x] List, read, overwrite, and forget memories.

## Independent memory curator

- [x] Run curation after each completed, non-interrupted turn.
- [x] Give the curator only the new turn plus a compact memory index.
- [x] Use typed `save` and `delete` edits.
- [x] Reject invalid edits and cap writes at five per turn.
- [x] Preserve the original 1024-token curator output cap.
- [x] Overwrite an existing memory to correct it.
- [x] Keep curator failures from taking down the coding session.
- [x] Keep memory-writing tools out of the coding Agent's tool roster.

## Tests and verification

- [x] Port deterministic filesystem-tool tests.
- [x] Port queue and interruption-classification tests.
- [x] Port memory parsing, persistence, search, injection, and curator-plumbing
  tests.
- [x] Add framework tests proving both provider entrypoints construct the same
  task/tool contract with different providers.
- [x] Compile the complete `baml_src_temp2` tree.
- [x] Run the complete offline temp2 test suite.
- [x] Run live OpenAI and Google Agent smoke tests when both credentials are
  available.

## Verification record

- `baml check`: all 215 temp2 source files compile.
- Scenario suite: 49 passed, 0 failed.
- Broad offline suite (`-x integ-test`): 205 passed, 0 failed.
- Live OpenAI Responses smoke: one native `list_dir` call followed by a typed
  answer.
- Live Google AI Gemini smoke: the same Task, tool roster, and typed answer
  after `Task.with_provider(...)`.
- Interactive OpenAI REPL: opened in a TTY and exited through the `exit`
  command.
