# BEP phase 1 — working reference

This tree implements the BEP in `../pages/` with code that actually runs:
the loop, the clients, and the how-tos are executable, and every design
claim listed below is covered by an offline test or a live smoke run.

The core library now ships as builtin stdlib packages compiled into the
toolchain (`crates/baml_builtins2/baml_std/{ai,openai,anthropic,google,
claude_code,mcp}`), reachable from any project as `ai.*`, `openai.*`,
`anthropic.*`, `google.*`, `claude_code.*`, and `mcp.*` — no `root.`
prefix and no files to copy. This tree keeps only the fixtures, tests,
how-tos, and live smokes, all written against those builtins.

There is one execution path: every LLM function desugars to the ai
world. Each gets a compiler-generated `<Fn>$spec` companion —
`Fn@spec(args)` builds the bound, unrun `ai.FunctionSpec<Out>` (see
`examples/plan_trip.baml`) — and its direct call desugars to
`ai.Agent<Out>.new(client = client).run(Fn@spec(...)).value`, with
`client` a compiler-injected `ai.Client? = null` override parameter.
`tools` is optional data (default: empty toolbox). The `client:` field
takes a `"provider/model"` string (compile-time-mapped; unknown prefixes
are errors) or any expression evaluating to `ai.Client`, and
`client Name = <expr>;` declares a named client value initialized once
at program start. The legacy forms — `client<llm>` blocks,
`retry_policy`, and Jinja `#"..."#` prompts — are
compile errors with migration hints. Companions: `$spec`,
`$render_prompt` (structural `ai.Prompt`), `$parse`, and
`$stream` (StreamingClient providers). Provider construction is pure:
credentials resolve from the environment at request time, so building
specs and declaring clients never touches env.

## Prompt rendering contract

`ai.FunctionSpec.prompt_template` holds a `(string) -> ai.Prompt` template closure.
`spec.prompt(output_format = output_format)` invokes that closure, and
`<Fn>$render_prompt(...)` returns the same structural prompt with the function's
output schema supplied automatically. There is no `ai.Prompt.render_text`.

The compiler lowers an LLM function's backtick prompt through the same
tagged-template assembler as the builtin `prompt` tag:

- template parts remain ordered;
- `${role("system")}`, `${role("user")}`, and other role markers start new
  prompt messages;
- interpolated media remains structural in the Rust-backed prompt data;
- ordinary values retain normal interpolation and `baml.ToString` behavior.

`Prompt.messages()` is the provider-facing role/content projection and
`Prompt.text()` is the readable, role-headed projection for display or a
single-string transport. OpenAI, Anthropic, and Google lower messages into
their own wire shapes. Claude Code alone calls `.text()`, at the final CLI
boundary because `claude -p` accepts one prompt string.

## Layout

```
baml_src/
├── examples/plan_trip.baml   the travel-agent fixture: PlanTrip is a real
│                             LLM function (client/tools/prompt); tests use
│                             PlanTrip@spec(...)
├── examples/logging.baml     log_event: every journal event as one log line
├── howto/                    the how-to pages as running code (parse feedback,
│                             attach_mcp)
├── tests/                    offline: ScriptedClient (test scaffolding,
│                             root namespace, not part of ai.*) drives
│                             everything; tests/mcp.baml fakes an MCP server in sh
├── live/                     live_openai() / live_anthropic() / live_google() /
│                             live_claude_code() / live_mcp_tools() /
│                             live_claude_code_dynamic_mcp()
└── resolve.baml              "prefix/model" -> client value (root, not ai)
```

## Running

Use `../target/debug/baml-cli`, not the brew `baml`: the int-to-float
widening in `reflect.call_any` and `baml.sys.start_process` exist only
on this branch, and the released toolchain loops fatally on float tool
arguments without the widening.

```bash
cd _planv2
../target/debug/baml-cli check
../target/debug/baml-cli test -x "live::"    # offline tests, no network
infisical run --env=test -- ../target/debug/baml-cli run -e 'live_openai()'
infisical run --env=test -- ../target/debug/baml-cli run -e 'live_anthropic()'
infisical run --env=test -- ../target/debug/baml-cli run -e 'live_google()'
../target/debug/baml-cli run -e 'live_claude_code()'   # uses the CLI's own login

# minimal integration gate: one tool loop through every configured provider
infisical run --env=test -- ../target/debug/baml-cli run --output-format json \
  -e '[live_openai(), live_anthropic(), live_google(), live_claude_code()]'

# MCP, both forms (no API key; npx fetches the everything server):
../target/debug/baml-cli run -e 'live_mcp_tools()'              # mcp: journaled tools
../target/debug/baml-cli run -e 'live_claude_code_dynamic_mcp()' # harness: model attaches mid-run

# the observable demo: every journal event as a log line
../target/debug/baml-cli test --logs INFO -i "live::claude code tool loop"
```

All three live smokes complete a real tool loop (>= 1 `ToolCompleted`) and
return a typed `Itinerary`.

## What the tests prove

- The scripted loop: tool calls execute concurrently, results correlate by
  id, the final candidate parses as the return type.
- Tool errors report to the model by default; the journal records the
  failure the model saw.
- `StepBudgetExceeded` is thrown typed and catchable.
- The within-turn repair loop: a Complete-but-unparseable turn triggers a
  committed re-ask — the failed attempt, its usage, and the correction
  request are journal events, and a repair attempt does not consume a step.
- The parse-feedback how-to works from public primitives only
  (`spec.prompt_template`, `Journal.append_all`, `UserMessage`, `client.invoke`).
- `@spec().prompt(...)` and `$render_prompt` return `ai.Prompt` values
  with authored message roles intact; media survives in the underlying Rust
  AST. Provider request-shape tests prove OpenAI, Anthropic, and Google consume
  those messages instead of receiving one flattened instruction string.
- `Retry` replays Safe network failures, never resends a rejected request;
  `Fallback` advances past a dead member. `FlakyClient` in the tests is the
  minimal custom client.
- `resolve("prefix/model")` builds the right provider client.
- The Claude Code envelope offers final-result-or-calls with `$defs`
  lifted, and the transcript folding marks successful tool results —
  the live smoke proves BAML tools execute through the harness.
- The MCP connection speaks the protocol end-to-end against a fake stdio
  server written in sh (handshake, catalog, call), and MCP calls land in
  the journal as `ToolRequested`/`ToolCompleted` like any tool. Live:
  `live_mcp_tools()` echoes through a real server with the Claude Code
  client; `live_claude_code_dynamic_mcp()` proves the model can attach a
  server mid-run via the `attach_mcp` tool and use its tools natively on
  the next turn (verified: `--allowedTools=mcp__<name>` is required, and
  the equals form matters because the flag is variadic and would swallow
  the trailing prompt argument).

## Reference implementation status

The BEP pages incorporate the reference-derived behavior below: structural
prompt rendering, provider-specific lowering, pure client construction,
committed repair events, boundary decoding, and the concrete event fields.
Items explicitly described as unimplemented are remaining implementation gaps;
they do not replace the intended BEP surface.

1. `client` is a keyword: `spec.client()` cannot exist; the impl exposes the
   `default_client` field. The `Agent.client` FIELD works everywhere,
   including `baml fmt` (the formatter crash is fixed on canary).
   `default_client` is an eager `Client` value: provider construction is
   pure (credentials resolve from the environment at request time, inside
   `invoke`), so building a spec — or declaring a client — never reads
   env and never panics on a missing key.
2. `Tool.on_error` is `ErrorMode?` where null inherits the run's
   `tool_errors`; this is what makes "per-tool wins" coherent.
3. `ToolUse.args` is `map<string, unknown>` and `ToolCompleted.output` /
   `FinalProduced.value_json` are JSON strings — not `json`-typed fields.
4. `FunctionSpec.prompt(...)` and `$render_prompt` return the Rust-backed
   `ai.Prompt`; `render_text` is gone. Role markers remain ordered
   messages and interpolated input media remains structural. `messages()` is
   used for provider-specific role lowering; `.text()` is an explicit readable
   projection. The BEP retains `Media` as model-output content and its phase 2
   return-type binding; the working reference does not implement that output
   path yet.
5. Wire APIs constrain how prompt messages and a journal may combine. OpenAI
   prepends the rendered prompt messages to Responses `input`. Anthropic moves
   authored system messages to its top-level `system` field and sends the
   remaining prompt messages before the journal; when the resulting transcript
   would not begin with a user message, system content becomes the leading user
   fallback. Google maps assistant to `model`, uses `systemInstruction` for
   authored system messages, and likewise uses a leading-user fallback for a
   system-only or assistant-first prompt. These policies live in the provider
   clients rather than in `ai.Prompt`.
6. The intended public surface remains `ai.clients.resolve/register`. In the
   working reference, `resolve` is a root fixture so the core namespace does
   not depend on providers, and `register()` remains deferred because it needs
   process-global state.
7. Model defaults are real models: `gpt-4o-mini`, `claude-haiku-4-5`
   (3-5-haiku is retired), `gemini-2.5-flash`.
8. `reflect.call_any` performs a boundary decode on dynamic arguments
   (`decode_for_param` in `bex_vm/package_reflect`). Integral JSON numbers
   widen into `float` and `float?` parameters (models send `150` for
   `150.0`, and JSON Schema `number` accepts both; without it Gemini loops
   to death on a retried invalid call), and a JSON array rebuilds
   element-wise into a typed array parameter (`json[]` binds `string[]`;
   the rule recurses, so integral elements widen inside `float[]`) —
   B-1174. Every conversion is lossless-only and constructs a fresh value:
   the float round-trip check admits integers up to 2^53 and rejects
   anything f64 cannot represent exactly, and a rebuilt array is a new
   allocation, so neither case is a type-level coercion and arrays stay
   invariant. The interim BAML-side workaround was removed;
   `tests/float_widening.baml` proves the boundary behavior directly.
9. The runner accepts a turn when it has tool calls OR parses; the repair
   budget is fixed at 2 re-asks per step (no `max_parse_attempts` knob, per
   the BEP). Repair attempts are committed events: the journal is the
   complete record, and `Journal.with` was removed in favor of a public
   `append_all` whose only writer is the driving runner.
10. Reasoning blocks are dropped when re-lowering assistant turns
    (Anthropic rejects unsigned thinking blocks) — the phase 2 replay
    capsule story, confirmed on the wire.
11. `ScriptedClient` is test scaffolding in `tests/` (root namespace), not
    a member of the public `ai` surface as the BEP's tree showed.
12. `ToolFailedError.cause` is `Failure?` rather than the reference's
    `(Failure | UnknownError)?`: an untyped cause carries in `message` as
    text and `cause` stays null.
13. `Retry` implements the documented `Backoff` (exponential with a cap,
    `retry_after_ms` hint override, `baml.sys.sleep`).
14. The failure taxonomy is its own namespace: `ai.errors`
    (baml_std/ai/ns_errors/), mirroring `baml.errors` — `ai.errors.Failure`,
    `classify_http`, and
    the classified classes.
15. `ClaudeCodeClient` is a harness client over `baml.sys.start_process`
    (ported from the old repo; streams the CLI's full stream-json event
    transcript live as log lines) — no HTTP.
    The output contract is native (`--json-schema`, `render("").text()`), BAML
    tools ride the `outcome` envelope, and two protocol lines are required
    in practice: "no tool results yet means outcome MUST be calls" (haiku
    otherwise answers directly and invents data) and "catalog tools are NOT
    installed; request via outcome.calls only" (the inner agent otherwise
    attempts native tool_use calls and burns an internal error round before
    finding the envelope). `session_id` + `--resume`
    is the phase 3 continuation candidate. The runner still drives the
    loop — the harness's inner episode is an inside-the-turn capability —
    but `max_steps` counts envelope rounds, and with `harness_tools`
    non-empty a mid-run failure should classify `Unknown` rather than
    `Safe` (not yet implemented).

## Language findings (kept in `baml describe`-verifiable form)

- No catch-arm guards; guard inside the arm with an if/else expression.
- `?.` chaining works on class-typed optionals — fields, methods, and
  through `.at(0)` (`resp.candidates?.at(0)?.content?.parts`). Two
  boundaries: `?.method()` on an interface-typed optional trips a VM
  error (B-1180; use if-let there), and `?? []` needs an annotated
  binding because the empty literal does not take its element type from
  the other operand (B-1181). One more rule: `?.` does not narrow the
  rest of the chain — after one optional link, every later link must
  also be `?.` even when the member is non-optional
  (`at(1)?.journal?.entries()?`, not `at(1)?.journal.entries()`;
  B-1182).
- `list.at(i)` accepts negative indices JS-style: in range they wrap
  from the end (`at(-1)` on a non-empty list is the last element), and
  out of range they return null, so `at(length - 1)` needs no
  empty-list guard.
- Lambdas over union-typed arrays need parameter annotations.
- A lambda calling a throwing function fails to unify with `throws never`
  parameters; wrap in `catch_all` inside the lambda.
- `string.substring(start, end)`, not `slice`.
- Struct spread `T { ...base, f: v }` exists and copies.
- `match` accepts string-literal arms (`"text" => ...`), not only type
  patterns and enum variants.
- Template literals trim leading/trailing whitespace, including escape
  sequences at the edges: `` `${line}\n` `` loses the newline. For
  protocol framing, append with a quoted string: `[line, "\n"].join("")`
  (`string` has no `concat`).
- A closure created in a `for` loop captures the loop variable's slot,
  not its value: every closure sees the final iteration. Mint the
  binding through a helper function parameter (see `mcp._proxy`).
- `reflect.call_any` decodes a JSON array into a typed array parameter
  (B-1174, fixed on this branch): `json[]` binds `string[]`, integral
  elements widen inside `float[]`, and a lossy element still throws.
  The decode builds a fresh `T[]` — not a subtyping rule; BAML arrays
  stay invariant, and covariance is unsound for mutable arrays.
  `tests/float_widening.baml` proves the boundary behavior. `raw_tool`
  remains for tools whose schema no signature derives (`mcp`).
