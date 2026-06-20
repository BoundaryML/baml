// =============================================================================
// implement.baml — Composing a durable step graph.
//
// Library / std-lib author code. Builds on the canonical primitives from
// _conventions.md (Provider, HttpProvider, Streaming, Tools).
//
// THE CENTRAL CLAIM OF THIS SCENARIO:
//   There is almost nothing to implement. A "step graph" — the Mastra
//   `.then/.parallel/.branch/.dountil` chain, the pydantic-graph node DAG, the
//   LangGraph StateGraph — is, in the proposed model, just ORDINARY BAML:
//     * a step           = a typed BAML `function`
//     * sequential edge  = `let b = StepB(StepA(x))`
//     * a branch         = `if` / `match`
//     * a loop           = `while` / `for`
//     * a model call in a step = `provider.call<T>(prompt)` — nothing special
//   So there is no `Step` interface, no `Workflow` combinator, no graph IR to
//   define here. The provider model is untouched: a step that talks to an LLM
//   just holds a Provider and calls it.
//
//   FAN-IN with real concurrency — starting N model calls and joining them (the
//   sync barrier that every framework "spends its design budget on", per
//   background §what's hard) — is NOT net-new: BAML has first-class structured
//   concurrency (BEP-034). `spawn { body }` starts `body` concurrently and
//   returns a `baml.future.Future` immediately; `await f` joins it. To express
//   `.parallel([summarize, classify])` you spawn each branch and join with
//   `baml.future.all`. Bounded fan-out (`.foreach concurrency:N`) is a
//   `baml.spawn.TaskGroup`. Cancellation is `Future.cancel()` /
//   `baml.spawn.CancelToken`. None of this is a Provider capability — it is the
//   language's own concurrency surface, and it is already shipped.
//
//   The ONE axis the title demands that IS net-new is DURABLE. A step graph is
//   durable iff each step's result is checkpointed so a re-run skips completed
//   steps. We model that as a `Checkpoint` store the step functions consult; it
//   wraps step EXECUTION, never the Provider. We show it here to prove the
//   proposal does not need to grow a durability capability: durability is
//   orthogonal to `call` (and orthogonal to concurrency).
//
// Net-new surface this file introduces (flagged in evaluation.md):
//   - `Checkpoint` — a content-addressed step-result store; makes the graph
//     re-runnable / durable. Wraps step execution, NOT the provider. This is the
//     ONLY genuinely net-new primitive in this scenario.
//   - `step<T>(...)` helper — memoizes one step through a Checkpoint. Pure glue.
//   Everything else — including the concurrency surface (`spawn`/`await`/
//   `baml.future.all`/`baml.spawn.TaskGroup`), Provider, HttpProvider, Tools —
//   is reused verbatim / is a shipped language feature.
// =============================================================================


// -----------------------------------------------------------------------------
// 0. Concurrency is a SHIPPED language feature (BEP-034) — nothing to add.
//
//    A fan-out + sync-barrier join (Mastra `.parallel`/`.foreach`,
//    pydantic-graph's parallel branches) is expressed directly:
//
//      let fa = spawn { branch_a() };               // starts concurrently
//      let fb = spawn { branch_b() };
//      let (a, b) = (await fa, await fb);           // join (the sync barrier)
//
//    Over a list, `baml.future.all` is the join:
//
//      let fs  = thunks.map(t => spawn { t() });
//      let xs  = await baml.future.all(fs);         // values in input order
//
//    `baml.future.all` is FAIL-FAST (JS `Promise.all`): the first future to fail
//    cancels the losers and re-throws. When losers have side effects that must
//    finish, `baml.future.all_complete` lets them run to completion (and still
//    throws the first error, in input order). Bounded fan-out uses a
//    `baml.spawn.TaskGroup` (the rate limiter): spawns sharing a group are
//    capped, excess queue FIFO. So fan-out is "a list of spawns", the join is
//    `baml.future.all`, and the "what does 'all' mean when one fails" question is
//    a choice between `all` (fail-fast) and `all_complete` (all-finish) — all of
//    it first-class in the language, none of it a Provider capability and none of
//    it net-new.
// -----------------------------------------------------------------------------


// -----------------------------------------------------------------------------
// 1. Durability: a Checkpoint store + a `step` memoizer.
//
//    A graph is "durable" iff a re-run after a crash skips steps that already
//    finished. The mechanism is dead simple and has NOTHING to do with the
//    Provider: key each step by (run_id, step_name, hash(input)); if the key is
//    present, return the stored output instead of re-executing.
//
//    Crucially this wraps STEP EXECUTION, not `provider.call`. The provider
//    stays single-shot and stateless. This is why the proposal does NOT need a
//    "durable" capability: durability is a property of the orchestration layer
//    (the step function), exactly like retry/fallback live in combinators.
// -----------------------------------------------------------------------------

interface Checkpoint {
  // Look up a previously-recorded step output by its content-addressed key.
  // null => not yet run (or evicted); the step must execute.
  function load<T>(self, key: string) -> T? throws baml.errors.Io
  // Record a completed step's output so a later re-run skips it.
  function save<T>(self, key: string, value: T) -> null throws baml.errors.Io
}

// The trivial durable-execution helper. EVERY step in a durable graph is wrapped
// in this. It is pure glue: no provider, no capability — just "did we already do
// this?". `body` is the step's actual work (which may, inside, call a Provider).
//
// The key is content-addressed: same run, same step name, same input => same
// key => same cached result. This is what makes a crashed graph resumable from
// the middle, and what makes a deterministic re-run a no-op.
function step<T>(ckpt: Checkpoint, run_id: string, name: string, input: string, body: () -> T throws baml.ExtendUnknownError<baml.errors.CallError>)
    -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
  let key = run_id + "/" + name + "/" + baml.hash.sha256(input);
  let cached: T? = ckpt.load<T>(key) catch (e) { _ => { null } };   // a checkpoint read failure just forces recompute
  match (cached) {
    let hit: T => { return hit; }     // already done — skip the work (durability)
    null => {
      let out = body();               // run the step (may do provider.call inside)
      ckpt.save<T>(key, out) catch (e) { _ => { null } };   // best-effort persist
      out
    }
  }
}


// -----------------------------------------------------------------------------
// 2. A concrete Checkpoint backend (host-backed, like PrimitiveClient).
//
//    Two backends prove the store is swappable: an in-memory one for tests and a
//    durable KV one for production. Both are just `implements Checkpoint` with
//    $rust_io bodies — the supported host-bridge boundary (Open Question 7).
// -----------------------------------------------------------------------------

class MemoryCheckpoint {
  _table: $rust_type
  function new() -> MemoryCheckpoint { MemoryCheckpoint { _table: {} } }
  implements Checkpoint {
    function load<T>(self, key: string) -> T? throws baml.errors.Io { $rust_io_function }
    function save<T>(self, key: string, value: T) -> null throws baml.errors.Io { $rust_io_function }
  }
}

class KvCheckpoint {
  namespace: string
  _kv: $rust_type            // a durable KV handle (Redis, SQLite, Durable Object…)
  implements Checkpoint {
    // load/save serialize T through the KV. Survives process restarts — this is
    // what makes the graph durable across a redeploy, not just a crash.
    function load<T>(self, key: string) -> T? throws baml.errors.Io { $rust_io_function }
    function save<T>(self, key: string, value: T) -> null throws baml.errors.Io { $rust_io_function }
  }
}


// -----------------------------------------------------------------------------
// 2b. Tiny `json` navigation helpers (reading a response envelope).
//
//    `baml.json.parse(resp.text())` yields the `json` union
//    (`null | bool | int | float | string | json[] | map<string, json>`).
//    `baml.json.field(j, key)` is the real stdlib accessor for the map arm;
//    these two helpers cover the array-index and string-extract arms so the
//    provider `parse` bodies read like `choices[0].message.content`.
// -----------------------------------------------------------------------------

function json_at(j: json, index: int) -> json {
  match (j) {
    let xs: json[] => match (xs.at(index)) { null => null, let v: json => v }
    _ => null
  }
}

function json_as_string(j: json) -> string {
  match (j) {
    let s: string => s
    _ => ""
  }
}


// -----------------------------------------------------------------------------
// 3. Two divergent chat providers, used INSIDE steps.
//
//    The scenario's portability proof: a step's model call is `p.call<T>(...)`,
//    and the two providers diverge exactly where the background docs say chat
//    models diverge — the structured-output ENCODING. OpenAI puts the schema in
//    `response_format`; Anthropic forces a tool whose input_schema IS the schema.
//    The step function never sees this: it writes `p.call<Summary>(prompt)`.
//
//    This is the whole point — the step graph reuses the Provider model UNCHANGED.
//    Nothing about being "in a graph" touches the provider.
// -----------------------------------------------------------------------------

class OpenAI {
  model: string
  api_key: string

  function gpt(model: string) -> OpenAI { OpenAI { model: model, api_key: env.OPENAI_API_KEY } }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError> {
      let schema = reflect.type_of<T>();
      // DIVERGENCE 1/2: OpenAI carries the schema in a top-level response_format.
      // The body is a native `map<string, json>` literal (nested values are native
      // json — strings, and the json fragments the wire-shapers return) serialized
      // with `baml.json.stringify`.
      let body: map<string, json> = {
        "model": self.model,
        "messages": baml.llm.render_messages(prompt),
        "response_format": baml.llm.openai_response_format(schema),
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "Authorization": "Bearer " + self.api_key, "Content-Type": "application/json" },
        body: baml.json.stringify(body),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai http failed"); } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Parse the envelope, navigate choices[0].message.content (a JSON-as-string
      // the model emitted), then SAP-coerce that string into T.
      let root = baml.json.parse(from.text());
      let content = baml.json.field(json_at(baml.json.field(root, "choices"), 0), "message");
      baml.sap.parse<T>(json_as_string(baml.json.field(content, "content")))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
  }
}

class Anthropic {
  model: string
  api_key: string
  max_tokens: int

  function claude(model: string) -> Anthropic {
    Anthropic { model: model, api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 }
  }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError> {
      let schema = reflect.type_of<T>();
      // DIVERGENCE 2/2: Anthropic has no response_format. We force a single tool
      // whose input_schema IS the output schema, then read the tool input back.
      // Body is a native `map<string, json>` literal: ints/strings are json, the
      // tool list is a json fragment, and `tool_choice` nests a sub-map literal.
      let body: map<string, json> = {
        "model": self.model,
        "max_tokens": self.max_tokens,
        "messages": baml.llm.render_messages(prompt),
        "tools": baml.llm.forced_schema_tool(schema),
        "tool_choice": { "type": "any" },
      };
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: { "x-api-key": self.api_key, "anthropic-version": "2023-06-01", "Content-Type": "application/json" },
        body: baml.json.stringify(body),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic http failed"); } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Read the forced-tool input back as the structured output. content[0].input
      // is already a JSON object (the model's tool input), so coerce that `json`
      // value into T directly with `from_json` — no re-stringify, no SAP needed.
      let root = baml.json.parse(from.text());
      let input = baml.json.field(json_at(baml.json.field(root, "content"), 0), "input");
      baml.json.from_json<T>(input)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
  }
}


// -----------------------------------------------------------------------------
// 4. The model-DECIDES loop is the Tools capability — contrast with code-defines.
//
//    The background doc draws the line: code-defines-the-graph (this whole file)
//    is plain functions and control flow; the model-decides loop is a DIFFERENT
//    thing — it is the agentic tool loop, which the proposal already models as
//    the `Tools` capability. We re-export NOTHING here; we just note that a step
//    in the graph may ITSELF be a model-decides loop by calling `.run_tools`.
//    The two compose: a code-defined graph whose one node is a model-driven
//    agent. No new surface — Tools is reused verbatim from _conventions.md.
//
//    We add ONE convenience so a graph node can be an agent in one line: a
//    minimal ExecutionContext that dispatches tools by a host registry. This is
//    not new capability surface — it is the same `ctx.dispatch` the Tools loop
//    already requires (proposal §7 net-new flag).
// -----------------------------------------------------------------------------

class ToolRegistryCtx {
  _registry: $rust_type     // name -> host fn, registered by the app
  implements ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // Sequential dispatch by default. A graph node that wants PARALLEL tool
      // dispatch spawns each call and joins with baml.future.all here (§0) — same
      // join barrier as .parallel, reused. We keep the default sequential for
      // determinism.
      let out: ToolResult[] = [];
      for (let c in calls) {
        let result = self.invoke_one(c) catch (e) { _ => { ToolResult { id: c.id, output: "tool error" } } };
        out.push(result);
      }
      out
    }
    function invoke_one(self, c: ToolCall) -> ToolResult throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
  }
}


// -----------------------------------------------------------------------------
// Host boundaries used above (genuine $rust seams; declared for completeness).
// -----------------------------------------------------------------------------
// spawn { } -> baml.future.Future ; await ; baml.future.{all,all_complete}     // SHIPPED structured concurrency (BEP-034)
// baml.spawn.TaskGroup.new(limit) ; spawn with baml.spawn.options(group=)       // bounded fan-out (the rate limiter)
// baml.sys.panic(msg) -> never
// baml.hash.sha256(s: string) -> string                        // content-addressed checkpoint keys (net-new)
// baml.llm.render_messages / openai_response_format / forced_schema_tool   // provider wire-shapers (return `json`)
// baml.sap.parse<T>(s: string) -> T
// Real stdlib used directly: `+` (string concat) · `xs.push(x)` · `xs.at(i)` (Array) ·
//   `{}` / `{ "k": v }` map literals · `baml.json.{parse,stringify,field,from_json}` ·
//   `baml.http.send` + inline header maps (no bearer/anthropic_headers sugar).
