# Claude trophy-flow transcripts

Question: When an agent run finishes, how does its result become a trophy? Name the function that parses the agent output and its input/output types, where those types are defined, where the trophy document is constructed, how the outcome is derived, and how Slack is notified. Include file:line citations.

| run | wall (s) | calls | BAML calls | grep/read calls | CLI output bytes | final bytes | tokens |
|---|---|---|---|---|---|---|---|
| claude/natural r1 | 51 | 8 | 0 | 5 | ? | ? | turns=9 cost=$0.5763975000000001 model=claude-haiku-4-5-20251001,claude-opus-4-8 |
| claude/natural r2 | 66 | 10 | 0 | 5 | ? | ? | turns=11 cost=$0.5149895 model=claude-haiku-4-5-20251001,claude-opus-4-8 |
| claude/natural r3 | 57 | 6 | 0 | 3 | ? | ? | turns=7 cost=$0.449511 model=claude-haiku-4-5-20251001,claude-opus-4-8 |
| claude/guided-hybrid-slim r1 | 77 | 7 | 3 | 2 | ? | ? | turns=8 cost=$0.39061199999999996 model=claude-haiku-4-5-20251001,claude-opus-4-8 |
| claude/guided-hybrid-slim r2 | 62 | 5 | 2 | 0 | ? | ? | turns=6 cost=$0.3249005 model=claude-haiku-4-5-20251001,claude-opus-4-8 |
| claude/guided-hybrid-slim r3 | 66 | 4 | 3 | 0 | ? | ? | turns=5 cost=$0.3085635 model=claude-haiku-4-5-20251001,claude-opus-4-8 |

## claude/natural r1 — commands
```
Grep	trophy|Trophy
Grep	parse.*agent|agent.*output|AgentOutput|parseAgent
Read	agent-tries-baml/baml_src/processors/worker.baml
Read	agent-tries-baml/convex/trophies.ts
Grep	class TrophyReport|class AgentResult|class TrophyIssue|function slack_post_message
Read	agent-tries-baml/baml_src/types.baml
Read	agent-tries-baml/baml_src/llm/worker.baml
Read	agent-tries-baml/baml_src/integrations/slack.baml
```

## claude/natural r1 — final answer

Here's the complete trace of how an agent run becomes a trophy.

## The flow

The whole thing runs inside `WorkerProcessor.process` at `baml_src/processors/worker.baml:78`. After the agent finishes (`self.proxy.run_agent(req)` → `AgentResult` at `worker.baml:148`), the result is turned into a trophy in five steps.

### 1. Parsing the agent output
**Function:** `parse_trophy(result: AgentResult) -> TrophyReport` — `baml_src/processors/worker.baml:236`

It pulls the agent's self-written `trophy.json` out of `result.post_files` and decodes it with `baml.json.from_string<TrophyReport>` (`worker.baml:241`). On any parse failure it falls back to an empty "no parseable trophy.json" report (`worker.baml:242-252`).

- **Input type `AgentResult`** — defined at `baml_src/types.baml:57` (status, token/cost metrics, `post_files`, `turn_log`, resolved `baml_version`, etc.)
- **Output type `TrophyReport`** — defined at `baml_src/llm/worker.baml:30` (its `issues` are `TrophyIssue`, `baml_src/llm/worker.baml:15`; the reporting spec the agent is told to follow is `trophy_spec()` at `worker.baml:41`)

Called from the process loop at `worker.baml:166`.

### 2. Deriving the outcome
**Function:** `derive_outcome(status: string, task_completed: string) -> string` — `baml_src/processors/worker.baml:256`

Logic: if `result.status != "ok"` → `"failed"`; else map the agent's self-reported `task_completed` → `"partial"` / `"success"` / `"failed"` (`worker.baml:257-266`). Invoked at `worker.baml:167` with `result.status` and `report.task_completed`.

### 3. Constructing the trophy document
**Type:** `TrophyDoc` — defined at `baml_src/processors/worker.baml:24`
**Constructed:** `worker.baml:170-210`

It assembles `outcome`, resolved `bamlVersion` (`result.baml_version` fallback to requested), `metrics` (via `metrics_of`, `worker.baml:350`), the report fields (`summary`, `whatWentWell`, `whatFailed`, `reportMd`), `findings` (via `findings_of` filtering `issues` to `kind == "skill" | "language"`, `worker.baml:450`), `filesCreated`, skill identity, transcript blob id, and `turnLog`.

It's serialized (`worker.baml:211`) and persisted via `self.svc.create("trophies", doc_json)` (`worker.baml:217`), which lands in the Convex `create` mutation at `convex/trophies.ts:51` → `lib.createDoc`. The task is then transitioned to `"done"` at `worker.baml:222`.

### 4. Slack notification
**Function:** `notify_slack_result(task, outcome, summary, result, findings_count, trophy_id) -> int` — `baml_src/processors/worker.baml:273`, called at `worker.baml:229`.

It no-ops when the task has no `slackChannel` or the bot token is unset (`worker.baml:286`). Otherwise it builds `*Result: <outcome>.* <summary>` + a stats line (turns · api · cost [· N issue(s) flagged]) + a `<link|view trophy>` pointing at `ui_base_url()/runs/<trophy_id>` (`worker.baml:310-324`), then posts it via `slack_post_message(token, channel, text, thread)` at `worker.baml:325`.

**`slack_post_message`** is defined at `baml_src/integrations/slack.baml:5` — a bearer-auth POST to `chat.postMessage`, replying in the bench thread (`thread_ts`) and returning the posted message `ts`.

## Summary map

| Concern | Where |
|---|---|
| Parse function | `parse_trophy` — `worker.baml:236` |
| Parse input type | `AgentResult` — `types.baml:57` |
| Parse output type | `TrophyReport` — `llm/worker.baml:30` |
| Trophy doc type | `TrophyDoc` — `worker.baml:24` |
| Trophy doc built | `worker.baml:170-210`, persisted `worker.baml:217` → `trophies.ts:51` |
| Outcome derived | `derive_outcome` — `worker.baml:256` (called `:167`) |
| Slack notify | `notify_slack_result` — `worker.baml:273` (called `:229`) → `slack_post_message` — `slack.baml:5` |

One note worth flagging: the file header (`worker.baml:6`) says Slack notify is "deferred (TODO)," but the code at `worker.baml:229`/`273` actually implements it — the comment is stale.

## claude/natural r2 — commands
```
Grep	trophy|Trophy
Grep	parse.*agent.*output|parseAgent|parse_agent|AgentOutput|agent output
Read	agent-tries-baml/convex/trophies.ts
Grep	trophy|Trophy|parseAgent|agentOutput|outcome
Read	agent-tries-baml/baml_src/core/transcript.baml
Read	agent-tries-baml/baml_src/processors/worker.baml
Grep	class TrophyReport|class AgentResult|class TrophyIssue|class TrophyRepro|class TrophySuggestion
Read	agent-tries-baml/baml_src/llm/worker.baml
Read	agent-tries-baml/baml_src/types.baml
Grep	function slack_post_message
```

## claude/natural r2 — final answer

Here's the full chain from an agent run finishing to a trophy document, all in the BAML worker processor.

## The flow

**1. Parse the agent output — `parse_trophy`**
- Defined at `baml_src/processors/worker.baml:236` — `function parse_trophy(result: AgentResult) -> TrophyReport`.
- It doesn't parse raw stdout; it reads the agent's self-authored `trophy.json` out of `result.post_files.get("trophy.json")` and decodes it with `baml.json.from_string<TrophyReport>` (`worker.baml:237-253`). On any parse failure it returns a stub empty report (`worker.baml:242-252`).
- **Input type** `AgentResult`: defined at `baml_src/types.baml:57` (the proxy's agent-run result — status, tokens, `post_files`, `turn_log`, `baml_version`, etc.).
- **Output type** `TrophyReport`: defined at `baml_src/llm/worker.baml:30` (with its nested `TrophyIssue` at `worker.baml:15`, `TrophyRepro` at `worker.baml:9`, `TrophySuggestion` at `worker.baml:24`). This is the schema the worker agent is instructed to write, per the `trophy_spec()` prompt at `baml_src/llm/worker.baml:41`.

**2. Derive the outcome — `derive_outcome`**
- Defined at `baml_src/processors/worker.baml:256`, called at `worker.baml:167`: `derive_outcome(result.status, report.task_completed)`.
- Logic (`worker.baml:257-266`): if the run `status != "ok"` → `"failed"`; else if `task_completed == "partial"` → `"partial"`; else if `task_completed == "true"` → `"success"`; otherwise `"failed"`.

**3. Construct the trophy document — the `TrophyDoc` literal**
- Built inline in the processor's `process` method at `baml_src/processors/worker.baml:170-210`.
- `TrophyDoc` type is defined at `baml_src/processors/worker.baml:24` (with `TrophyFinding` at `worker.baml:15` and `TrophyFindingAnchor` at `worker.baml:10`).
- It assembles the outcome, `metrics_of(result, self.model)` (`worker.baml:350`), the report fields (summary/whatWentWell/whatFailed/reportMd/suggestions), `findings_of(report)` (`worker.baml:450`, which keeps only `skill`/`language` issues), `filesCreated`, skill identity, the transcript blob id, and the turn log.
- It's serialized with `baml.json.to_string(doc)` (`worker.baml:211`) and persisted via `self.svc.create("trophies", doc_json)` (`worker.baml:217`), which is the Convex `trophies.create` mutation at `convex/trophies.ts:51`. The task is then transitioned to `done` (`worker.baml:222`).

**4. Notify Slack — `notify_slack_result`**
- Defined at `baml_src/processors/worker.baml:273`, invoked at `worker.baml:229` after the trophy is created: `notify_slack_result(task, outcome, report.summary, result, findings.length(), trophy_id)`.
- No-ops when the task has no `slackChannel` or the bot token is unset (`worker.baml:281-287`). Otherwise it builds `*Result: <outcome>.* <summary>` + a `turns · api · cost [· N issue(s) flagged]` stats line + a `<link|view trophy>` pointing at `ui_base_url()/runs/<trophy_id>` (`worker.baml:310-324`).
- The actual post goes out through `slack_post_message(...)` at `worker.baml:325`, defined at `baml_src/integrations/slack.baml:5`, threaded under `task.slackThreadTs`.

## One-line summary of the pipeline
`process` (`worker.baml:78`) → `parse_trophy` (`AgentResult`→`TrophyReport`) → `derive_outcome` → build `TrophyDoc` → `svc.create("trophies", …)` (`trophies.create`) → `notify_slack_result` → `slack_post_message`.

## claude/natural r3 — commands
```
Grep	trophy
Read	agent-tries-baml/convex/trophies.ts
Grep	trophy
Read	agent-tries-baml/baml_src/processors/worker.baml
Grep	class (AgentResult|TrophyReport)
Read	agent-tries-baml/baml_src/llm/worker.baml
```

## claude/natural r3 — final answer

I now have the complete flow. Here's how an agent run result becomes a trophy.

## The flow

Everything happens in `WorkerProcessor.process`, the worker's main loop, in `baml_src/processors/worker.baml:78`. After the proxy runs the agent (`self.proxy.run_agent`, line 148), the result is turned into a trophy in five steps.

### 1. Parsing the agent output

**Function:** `parse_trophy` — `baml_src/processors/worker.baml:236`
- **Input:** `AgentResult` — defined at `baml_src/types.baml:57`
- **Output:** `TrophyReport` — defined at `baml_src/llm/worker.baml:30`

It reads the `trophy.json` file the agent wrote into `result.post_files` and deserializes it with `baml.json.from_string<TrophyReport>` (line 241). On any parse failure it falls back to an empty "no parseable trophy.json" report (lines 242–252). Called at `worker.baml:166`.

The agent is *told* to produce this file by the `trophy_spec()` prompt at `baml_src/llm/worker.baml:41`, and `TrophyReport`'s sub-types (`TrophyIssue`, `TrophyRepro`, `TrophySuggestion`) live alongside it at `baml_src/llm/worker.baml:9–38`.

### 2. Constructing the trophy document

**Where:** the `TrophyDoc { ... }` literal at `baml_src/processors/worker.baml:170–210`.
- **Type:** `TrophyDoc` — defined at `baml_src/processors/worker.baml:24`.

It merges parsed-report fields (`summary`, `whatWentWell`, `whatFailed`, `reportMd`, `suggestions`) with proxy-derived data: `metrics` via `metrics_of` (line 179 / def line 350), `findings` via `findings_of` (line 168 / def line 450, which keeps only `kind == "skill"|"language"` issues), the transcript blob id (line 156), and the turn log (line 195). It's serialized with `baml.json.to_string` (line 211) and persisted via `self.svc.create("trophies", doc_json)` at `worker.baml:217` — returning `trophy_id`. The task is then transitioned to `done` (line 222).

### 3. Deriving the outcome

**Function:** `derive_outcome(status, task_completed) -> string` — `baml_src/processors/worker.baml:256`, called at line 167.

Logic: if `result.status != "ok"` → `"failed"`; else if `task_completed == "partial"` → `"partial"`; else if `task_completed == "true"` → `"success"`; otherwise `"failed"`. So the proxy's run status is authoritative for failure, and the agent's self-reported `task_completed` distinguishes success/partial.

### 4. Notifying Slack

**Function:** `notify_slack_result(task, outcome, summary, result, findings_count, trophy_id) -> int` — `baml_src/processors/worker.baml:273`, called at line 229.

It no-ops when `task.slackChannel` is empty or the bot token is unset (lines 281–288). Otherwise it builds a `*Result: <outcome>.* <summary>` message plus a stats line (`turns · api · cost [· N issue(s) flagged]`) and a `<{ui_base_url}/runs/{trophy_id}|view trophy>` link (lines 310–324), then posts it into the bench thread via `slack_post_message(token, channel, text, thread)` at line 325.

## Note

The persisted `TrophyDoc` (worker.baml:24) is the write-side shape; the read-side/CRUD `Trophy` class is separately defined at `baml_src/types.baml:170`, and the Convex CRUD surface for the `trophies` table is `convex/trophies.ts`.

## claude/guided-hybrid-slim r1 — commands
```
Bash	baml describe --grep "parse_trophy || parse_agent || trophy || derive_outcome || notify_slack || slack" --kind function --view source --agent --limit 6 --budget 60
Bash	baml describe AgentResult TrophyReport parse_trophy notify_slack_result --view source --agent --budget 90
Read	agent-tries-baml/baml_src/processors/worker.baml
Bash	baml describe parse_trophy --view usage --agent --budget 40; echo "==="; baml describe derive_outcome --view usage --agent --budget 30
Read	agent-tries-baml/baml_src/processors/worker.baml
Bash	sed -n '120,150p' baml_src/processors/worker.baml && echo "=== TrophyDoc ===" && rg -n "class TrophyDoc" baml_src
Bash	rg -n "^\s*(function|impl|class Worker|method)" baml_src/processors/worker.baml | head -20
```

## claude/guided-hybrid-slim r1 — final answer

The whole pipeline lives in `WorkerProcessor.process` (`baml_src/processors/worker.baml:78`). Here's how an agent run becomes a trophy:

## The pipeline

The agent runs via `self.proxy.run_agent(req)` (`worker.baml:147`), returning an `AgentResult`. From there:

**1. Parse the agent output — `parse_trophy`** (`baml_src/processors/worker.baml:236-254`)
- Signature: `parse_trophy(result: AgentResult) -> TrophyReport`, called at `worker.baml:166`.
- It reads the agent's self-written `trophy.json` out of `result.post_files.get("trophy.json")` and JSON-decodes it into a `TrophyReport`, falling back to an empty "no parseable trophy.json" report on failure.
- **Input type `AgentResult`** — defined at `baml_src/types.baml:57-77` (the proxy's run output: status, exit_code, transcript, token/cost metrics, and the `post_files` map).
- **Output type `TrophyReport`** — defined at `baml_src/llm/worker.baml:30-38` (report_md, task_completed, summary, what_went_well/what_failed, issues, suggestions). Its shape is dictated to the agent by `trophy_spec()` at `baml_src/llm/worker.baml:41-75`.

**2. Derive the outcome — `derive_outcome`** (`baml_src/processors/worker.baml:256-267`)
- Called at `worker.baml:167`: `derive_outcome(result.status, report.task_completed)`.
- Logic: `status != "ok"` → `"failed"`; else `task_completed == "partial"` → `"partial"`, `"true"` → success, otherwise the remaining case. It combines the *proxy's* run status with the agent's *self-reported* completion.

**3. Construct the trophy document — `TrophyDoc`** (`baml_src/processors/worker.baml:170-210`)
- The `TrophyDoc` class is defined at `baml_src/processors/worker.baml:24`.
- Built inline in `process`, assembling: `taskId`, `outcome`, `bamlVersion`, `metrics` (via `metrics_of`, `worker.baml:350`), the report fields, `findings` (via `findings_of`, `worker.baml:450`), `filesCreated`, transcript storage id, `turnLog`, and cohort/status.
- Serialized at `worker.baml:211` and persisted via `self.svc.create("trophies", doc_json)` at `worker.baml:217`, which returns the `trophy_id`. The task is then transitioned to `"done"` at `worker.baml:222`.

**4. Notify Slack — `notify_slack_result`** (`baml_src/processors/worker.baml:273-327`)
- Called at `worker.baml:229`: `notify_slack_result(task, outcome, report.summary, result, findings.length(), trophy_id)`.
- It resolves `task.slackChannel` and the bot token from `slack_bot_token()` (`baml_src/core/env.baml:294-296`, reading `SLACK_BOT_TOKEN`); if either is empty it returns `0` (no-op for non-Slack tasks). Otherwise it replies in the bench thread (`task.slackThreadTs`) with the outcome, summary, metrics, and a trophy link.

## Flow summary

```
proxy.run_agent(req) → AgentResult (worker.baml:147)
        │
        ├─ parse_trophy(result) ─────────────→ TrophyReport   (:166)
        │      reads result.post_files["trophy.json"]
        ├─ derive_outcome(result.status, report.task_completed) → outcome (:167)
        ├─ TrophyDoc { … } constructed        (:170-210)
        ├─ svc.create("trophies", doc_json) → trophy_id  (:217)
        ├─ svc.transition(task → "done")      (:222)
        └─ notify_slack_result(task, outcome, summary, …, trophy_id)  (:229)
```

Type definitions at a glance: `AgentResult` → `baml_src/types.baml:57`; `TrophyReport` → `baml_src/llm/worker.baml:30`; `TrophyDoc` → `baml_src/processors/worker.baml:24`.

## claude/guided-hybrid-slim r2 — commands
```
Bash	baml describe --grep "parse_agent || agent_output || trophy || derive_outcome || notify_slack || slack" --view source --agent --limit 8 --budget 60
Bash	baml describe --grep "parse_agent_result || build_trophy || AgentResult || make_trophy || parse_output" --view source --agent --limit 8 --budget 70
Bash	rg -n "AgentResult|TrophyDoc|derive_outcome|notify_slack_result|fn |function " baml_src/processors/worker.baml | head -60
Bash	rg -n "class TrophyReport" baml_src -r; echo "---"; sed -n '150,270p' baml_src/processors/worker.baml
Bash	rg -n "class TrophyReport" baml_src; echo "---notify body---"; sed -n '273,333p' baml_src/processors/worker.baml
```

## claude/guided-hybrid-slim r2 — final answer

Here's the full agent-result → trophy flow, traced through `baml_src/processors/worker.baml`.

## The parse function

**`parse_trophy(result: AgentResult) -> TrophyReport`** — `baml_src/processors/worker.baml:236-254`

It reads the agent's self-reported `trophy.json` out of `result.post_files`, deserializes it into a `TrophyReport`, and falls back to an empty report if the JSON is missing/unparseable.

- **Input type `AgentResult`** — defined at `baml_src/types.baml:57-77` (cell_id, status, exit_code, transcript, token/cost metrics, `post_files: map<string,string>`, etc.)
- **Output type `TrophyReport`** — defined at `baml_src/llm/worker.baml:30` (report_md, task_completed, summary, what_went_well, what_failed, issues, suggestions)

## Where the trophy document is constructed

Inside the processor's `process(self, raw)` method:

- `let report = parse_trophy(result);` — `worker.baml:166`
- The **`TrophyDoc`** literal is built at `worker.baml:170-211` (class `TrophyDoc` defined at `worker.baml:24-42`). It stitches together the derived outcome, metrics (`metrics_of`), the report fields, findings (`findings_of`), `post_files`, transcript storage id, turn log, and cohort/status.
- It's serialized (`baml.json.to_string`) and persisted via `self.svc.create("trophies", doc_json)` at `worker.baml:216`, which returns `trophy_id`.

## How the outcome is derived

`let outcome = derive_outcome(result.status, report.task_completed);` — `worker.baml:167`

**`derive_outcome(status, task_completed) -> string`** — `worker.baml:256-267`: returns `"failed"` if `status != "ok"`; otherwise maps `task_completed` → `"partial"` / `"success"`, defaulting to `"failed"`.

## How Slack is notified

`let _ = notify_slack_result(task, outcome, report.summary, result, findings.length(), trophy_id);` — `worker.baml:229` (called after the task is transitioned to `done`).

**`notify_slack_result(task, outcome, summary, result, findings_count, trophy_id) -> int`** — `worker.baml:273-327`: no-ops when the task has no `slackChannel` or the bot token is unset; otherwise formats a result line (outcome, summary, turns/api/cost, flagged-issue count, and a `/runs/<trophy_id>` trophy link) and posts it into the bench thread via **`slack_post_message(token, channel, text, thread)`** at `worker.baml:326`.

## Flow summary

```
result: AgentResult (types.baml:57)
  → parse_trophy (worker.baml:236) → report: TrophyReport (llm/worker.baml:30)
  → derive_outcome (worker.baml:256) → outcome
  → TrophyDoc {…} (worker.baml:170, class @24) → svc.create("trophies") (worker.baml:216) → trophy_id
  → notify_slack_result (worker.baml:273) → slack_post_message (worker.baml:326)
```

## claude/guided-hybrid-slim r3 — commands
```
Bash	baml describe --grep "parse_agent || parse_output || trophy || derive_outcome || notify_slack || slack" --view source --agent --limit 8 --budget 80
Bash	baml describe --grep "parse || extract || build_trophy || make_trophy || AgentResult || AgentOutput" --kind function --view source --agent --limit 8 --budget 90
Bash	baml describe AgentResult TrophyReport --view source --agent --budget 40
echo "=====WORKER FLOW====="
sed -n '1,120p' baml_src/processors/worker.baml
Bash	sed -n '160,330p' baml_src/processors/worker.baml
```

## claude/guided-hybrid-slim r3 — final answer

Here's the complete `AgentResult → Trophy` flow, all in `baml_src/processors/worker.baml` unless noted.

## Parse function
**`parse_trophy(result: AgentResult) -> TrophyReport`** — `worker.baml:236-254`. Reads the agent's self-reported `trophy.json` out of `result.post_files`, deserializing it via `baml.json.from_string<TrophyReport>`; on failure it returns an empty fallback report.

- **Input** `AgentResult` — defined in `baml_src/types.baml:57-77` (carries `status`, `post_files: map<string,string>`, token/cost metrics, `turn_log`, `baml_version`, etc.).
- **Output** `TrophyReport` — defined in `baml_src/llm/worker.baml:30-38` (`report_md`, `task_completed`, `summary`, `what_went_well[]`, `what_failed[]`, `issues[]`, `suggestions[]`).

## Outcome derivation
**`derive_outcome(status: string, task_completed: string) -> string`** — `worker.baml:256-267`. Called at `worker.baml:167`. Logic: non-`"ok"` status → `"failed"`; else `task_completed == "partial"` → `"partial"`, `"true"` → `"success"`, otherwise `"failed"`.

## Trophy document construction
Built in the processor's `process` method as a **`TrophyDoc { … }`** literal at **`worker.baml:169-211`** (the `TrophyDoc` class is defined at `worker.baml:24-42`). It stitches together `parse_trophy`'s report, the derived `outcome`, `metrics_of(result, self.model)`, `findings_of(report)`, the resolved `result.baml_version`, `filesCreated: result.post_files`, and skill/transcript/cohort metadata. It is then serialized (`worker.baml:211`) and persisted via `self.svc.create("trophies", doc_json)` at **`worker.baml:216`**, yielding `trophy_id`; the task is transitioned to `done` at `worker.baml:222`.

## Slack notification
**`notify_slack_result(task, outcome, summary, result, findings_count, trophy_id) -> int`** — defined `worker.baml:273-327`, invoked at **`worker.baml:230`**. No-ops when `task.slackChannel` or the `slack_bot_token()` is empty; otherwise it formats `*Result: <outcome>.* <summary>` plus a stats line (turns · api · cost [· N issue(s) flagged]) and a `<link|view trophy>` (link = `ui_base_url() + "/runs/" + trophy_id`), then sends it by calling **`slack_post_message(token, channel, text, thread)`** at `worker.baml:325`, replying in the bench thread (`task.slackThreadTs`).

Flow summary: `parse_trophy` (236) → `derive_outcome` (167/256) → `TrophyDoc` literal (169) → `svc.create("trophies", …)` (216) → task `done` (222) → `notify_slack_result` (230).
