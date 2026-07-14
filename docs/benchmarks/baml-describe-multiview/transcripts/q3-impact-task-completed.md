# agent-bench 20260714-133917

Question: What would likely need to change if TrophyReport.task_completed changed from a string to an enum?

| run | wall (s) | calls | BAML calls | grep/read calls | CLI output bytes | final bytes | tokens |
|---|---|---|---|---|---|---|---|
| claude/natural r1 | 37 | 3 | 0 | 2 | ? | ? | turns=4 cost=$0.2171085 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-grep r1 | 35 | 4 | 0 | 3 | ? | ? | turns=5 cost=$0.19345600000000002 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |
| claude/guided-hybrid-slim r1 | 53 | 4 | 2 | 2 | ? | ? | turns=5 cost=$0.15581699999999998 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m] |

## claude/natural r1 — commands
```
Grep	TrophyReport|task_completed
Read	agent-tries-baml/baml_src/llm/worker.baml
Read	agent-tries-baml/baml_src/processors/worker.baml
```

## claude/natural r1 — final answer

Here's what I found. `TrophyReport.task_completed` is a `string` today (documented as `"true" | "false" | "partial"`), and it flows: agent writes `trophy.json` → `parse_trophy` decodes it → `derive_outcome` maps it to an outcome. Changing it to an enum would touch these spots:

## 1. The type definition — `baml_src/llm/worker.baml:32`
```
task_completed: string, // "true" | "false" | "partial"
```
Add an `enum` (e.g. `TaskCompleted { True, False, Partial }` — note `true`/`false` aren't valid identifiers, so you'd need aliased/renamed variants) and change the field to `task_completed: TaskCompleted`.

## 2. `parse_trophy` fallback — `baml_src/processors/worker.baml:245`
The hardcoded fallback `task_completed: "false"` must become the enum variant (e.g. `TaskCompleted.False`). Also worth checking: `baml.json.from_string<TrophyReport>` now has to coerce the JSON string `"true"|"false"|"partial"` into enum variants. BAML enum parsing/aliasing needs to accept those exact lowercase string values, or malformed agent reports that currently just carry a bad string will now fail the parse and hit this fallback branch more often.

## 3. `derive_outcome` — `baml_src/processors/worker.baml:256-266`
- Signature `task_completed: string` → `task_completed: TaskCompleted`.
- The two string comparisons `== "partial"` (line 260) and `== "true"` (line 263) become enum comparisons/match arms. Since enums are exhaustive, this is a good place to convert to a `match` so all variants are handled.

## 4. The prompt spec — `baml_src/llm/worker.baml:48`
```
"task_completed": "true" | "false" | "partial",
```
This is the instruction the agent follows when writing `trophy.json`. The JSON on disk stays a string regardless, so the wire format doesn't change — but keep this in sync with the enum's accepted values so agent output still parses.

## 5. Test fixture — `docker/fake-claude:29`
```
task_completed: "true",
```
Emits a real `trophy.json`. Its string value just needs to remain one the enum can parse (`"true"` → `TaskCompleted.True`), so likely no change, but it's the thing to run to verify end-to-end.

## Also check
- `docs/reference.md:175` documents `_derive_outcome(agent_status, task_completed)` — update the signature description.
- Anything downstream that reads `report.task_completed` directly — grep shows the only consumer is `derive_outcome`; `outcome` (the derived string) is what gets stored in `TrophyDoc`/persisted, so the enum change stays contained to the parse-and-derive path and doesn't ripple into the trophy schema or dashboard.

**Bottom line:** it's a contained change (one type def + two consumers + one fixture + docs), but the real risk is at the parse boundary — `from_string` must reliably coerce the existing lowercase JSON values into the enum, or you silently reclassify reports as `failed` via the fallback.

Want me to make the change?

## claude/guided-grep r1 — commands
```
Grep	TrophyReport|task_completed
Read	agent-tries-baml/baml_src/llm/worker.baml
Read	agent-tries-baml/baml_src/processors/worker.baml
Read	agent-tries-baml/baml_src/llm/worker.baml
```

## claude/guided-grep r1 — final answer

## What would need to change

Changing `TrophyReport.task_completed` from `string` to an enum touches four places — the type declaration, the two consumers of the field, and the prompt/parse contract that produces the values.

**1. The enum + field declaration** — `baml_src/llm/worker.baml:30-32`
Add a new enum (e.g. `TrophyStatus` with variants for `true`/`false`/`partial`) and change the field:
```
task_completed: TrophyStatus,   // was string
```
Because the agent writes these as JSON *strings* (`"true"`, `"false"`, `"partial"`), the enum variants must deserialize from those exact strings. `true`/`false` aren't valid identifiers, so the variants will need aliases mapping to those literals for `baml.json.from_string` to parse them.

**2. `derive_outcome` — `baml_src/processors/worker.baml:256-267`**
This is the main logic consumer. Its parameter and comparisons are string-typed:
- signature `derive_outcome(status: string, task_completed: string)` → the second param becomes the enum type
- `task_completed == "partial"` and `task_completed == "true"` (lines 260, 263) → become enum-variant comparisons/match arms
- The caller at `worker.baml:167` (`derive_outcome(result.status, report.task_completed)`) keeps working as long as the param type is updated to match.

**3. `parse_trophy` fallback — `baml_src/processors/worker.baml:243-251`**
The hand-constructed fallback `TrophyReport { ... task_completed: "false", ... }` (line 245) uses a string literal; it must be the corresponding enum variant instead.

**4. The prompt contract — `baml_src/llm/worker.baml:32`, `48`, and the spec text**
Not a compile dependency, but a correctness one: the trophy spec instructs the LLM to emit `"true" | "false" | "partial"` (line 48, plus the inline comment on line 32). These must stay exactly in sync with the enum's accepted values/aliases, or every real run falls into the `catch` fallback in `parse_trophy` and silently reports `"false"`.

**Not affected:** `findings_of` (`worker.baml:450`) and the `TrophyDoc` construction (lines 170-210) don't read `task_completed` — only `derive_outcome` and the parse fallback do — so the downstream `outcome` string and serialization are unchanged as long as `derive_outcome` still returns its string outcomes.

The cheapest verification path if you make the change: it's the two literal comparisons in `derive_outcome`, the one fallback literal, and the prompt text — those are the spots that break the build or silently mis-parse if missed.

## claude/guided-hybrid-slim r1 — commands
```
Bash	baml describe TrophyReport.task_completed --view impact --output compact
Bash	baml describe derive_outcome --view source --output compact
Read	agent-tries-baml/baml_src/processors/worker.baml
Read	agent-tries-baml/baml_src/processors/worker.baml
```

## claude/guided-hybrid-slim r1 — final answer

## What would need to change

Changing `TrophyReport.task_completed` from `string` to an enum touches **four spots, all in `baml_src/processors/worker.baml`**, plus the type definition itself:

**1. The `TrophyReport` type definition** — declare the new enum and change the field. The current data uses three string values (`"true"`, `"false"`, `"partial"`), so the enum needs those three variants.

**2. `derive_outcome` signature — line 256**
```
function derive_outcome(status: string, task_completed: string) -> string
```
The `task_completed: string` parameter must become the enum type.

**3. `derive_outcome` body — lines 260 & 263**
```
if (task_completed == "partial") { ... }
if (task_completed == "true")    { ... }
```
These string-literal comparisons must become enum-variant comparisons. (The `status` checks are unaffected — that's a separate string.)

**4. The fallback literal — line 245**
```
task_completed: "false",
```
Inside `parse_trophy`'s catch fallback, this string literal must become the corresponding enum variant.

**5. The call site — line 167** (`derive_outcome(result.status, report.task_completed)`) doesn't need editing *per se*, but it's the point where the enum-typed field now flows into `derive_outcome`; it only stays valid because you're changing #2/#3 in lockstep.

## One thing worth flagging

At line 241, `parse_trophy` deserializes external JSON via `baml.json.from_string<TrophyReport>(raw)`. With a `string` field, any value parses; with an enum, a JSON `task_completed` value outside your defined variants will fail to deserialize. That failure is currently swallowed by the `catch` on line 241 (falling back to the default report), so it won't crash — but agents reporting an unexpected value would silently collapse to the fallback "no parseable trophy.json" report instead of preserving their other fields. Consider whether the enum needs an `@@dynamic`/catch-all variant or lenient parsing.

The impact is well-contained: everything is in `worker.baml`, and `derive_outcome` is the only logic that inspects the value.
