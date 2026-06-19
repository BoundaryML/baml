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
//   The ONE thing the host language does NOT give you for free is FAN-IN with
//   real concurrency: starting N model calls and joining them (the sync barrier
//   that every framework "spends its design budget on", per background §what's
//   hard). BAML control flow is sequential; `let a = ...; let b = ...;` runs a
//   then b. To express `.parallel([...])` honestly we need a host primitive for
//   structured concurrency. THAT is the only net-new surface here, and it is a
//   host scheduler seam, NOT a provider capability.
//
//   The SECOND axis the title demands — DURABLE — is also not a provider
//   concern. A step graph is durable iff each step's result is checkpointed so a
//   re-run skips completed steps. We model that as a `Checkpoint` store the step
//   functions consult; it wraps step EXECUTION, never the Provider. We show it
//   here to prove the proposal does not need to grow a durability capability:
//   durability is orthogonal to `call`.
//
// Net-new surface this file introduces (flagged in evaluation.md):
//   - `baml.async.join2 / join_all` — host structured-concurrency seam: the ONLY
//     way to get a real parallel fan-out + sync-barrier join. Not a Provider
//     thing. (background §what's hard: "fan-in is where complexity hides")
//   - `Checkpoint` — a content-addressed step-result store; makes the graph
//     re-runnable / durable. Wraps step execution, NOT the provider.
//   - `step<T>(...)` helper — memoizes one step through a Checkpoint. Pure glue.
//   Everything else (Provider, HttpProvider, Tools) is reused verbatim.
// =============================================================================


// -----------------------------------------------------------------------------
// 0. The host structured-concurrency seam.
//
//    This is the irreducible net-new primitive. BAML expression evaluation is
//    sequential: `let a = f(); let b = g();` evaluates f to completion before g.
//    Fan-out (`.parallel`, `.foreach`) needs to START both and join them — a
//    sync barrier. No provider interface can supply this; it is the SCHEDULER's
//    job. So we expose it as a host async primitive, deliberately OUTSIDE the
//    Provider hierarchy.
//
//    A thunk is a zero-arg closure `() -> T throws E` — the standard way BAML
//    defers a computation. join2/join_all run the thunks concurrently and block
//    (the sync barrier) until ALL complete, then return the tuple/array.
//
//    `join_all` also fixes the failure-mode question every framework answers
//    ("what does 'all' mean when one fails"): it is FAIL-FAST — the first thunk
//    to throw aborts the others and re-throws. The all-settled variant
//    (collect every Result, succeed even if some fail) is `join_settled`.
// -----------------------------------------------------------------------------

// baml.async  (host seam — structured concurrency over thunks)
//   function join2<A, B>(a: () -> A throws E, b: () -> B throws E) -> (A, B) throws E
//   function join_all<T>(thunks: (() -> T throws E)[]) -> T[] throws E          // fail-fast
//   function join_settled<T>(thunks: (() -> T throws E)[]) -> Result<T, E>[]    // all-settled
//   function map_concurrent<I, O>(items: I[], width: int, f: (I) -> O throws E) -> O[] throws E
//                                                                               // .foreach with bounded concurrency

// A Result is the all-settled cell: exactly one of value/error is present.
class Result<T, E> {
  value: T?
  error: E?
  ok: bool
}


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
function step<T>(ckpt: Checkpoint, run_id: string, name: string, input: string, body: () -> T throws baml.errors.LlmClient)
    -> T throws baml.errors.LlmClient {
  let key = baml.str.concat(run_id, baml.str.concat("/", baml.str.concat(name, baml.str.concat("/", baml.hash.sha256(input)))));
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
  function new() -> MemoryCheckpoint { MemoryCheckpoint { _table: baml.sys.new_map() } }
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      let schema = reflect.type_of<T>();
      // DIVERGENCE 1/2: OpenAI carries the schema in a top-level response_format.
      baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: baml.http.bearer(self.api_key),
        body: baml.json.encode(baml.json.object([
          ("model", baml.json.string(self.model)),
          ("messages", baml.llm.render_messages(prompt)),
          ("response_format", baml.llm.openai_response_format(schema)),
        ])),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "openai http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("choices.0.message.content"))
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      let schema = reflect.type_of<T>();
      // DIVERGENCE 2/2: Anthropic has no response_format. We force a single tool
      // whose input_schema IS the output schema, then read the tool input back.
      baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: baml.http.anthropic_headers(self.api_key),
        body: baml.json.encode(baml.json.object([
          ("model", baml.json.string(self.model)),
          ("max_tokens", baml.json.int(self.max_tokens)),
          ("messages", baml.llm.render_messages(prompt)),
          ("tools", baml.llm.forced_schema_tool(schema)),
          ("tool_choice", baml.json.object([("type", baml.json.string("any"))])),
        ])),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "anthropic http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // Read the forced-tool input back as the structured output.
      baml.sap.parse<T>(from.json_path("content.0.input"))
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
    function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.errors.LlmClient {
      // Sequential dispatch by default. A graph node that wants PARALLEL tool
      // dispatch uses baml.async.map_concurrent here (§0) — same join barrier as
      // .parallel, reused. We keep the default sequential for determinism.
      let out: ToolResult[] = [];
      for (let c in calls) {
        let result = self.invoke_one(c) catch (e) { _ => { ToolResult { id: c.id, output: "tool error" } } };
        out = baml.list.push(out, result);
      }
      out
    }
    function invoke_one(self, c: ToolCall) -> ToolResult throws baml.errors.LlmClient { $rust_io_function }
  }
}


// -----------------------------------------------------------------------------
// Host boundaries used above (genuine $rust seams; declared for completeness).
// -----------------------------------------------------------------------------
// baml.async.join2 / join_all / join_settled / map_concurrent  // STRUCTURED CONCURRENCY — the net-new seam
// baml.sys.new_map() -> $rust_type ; baml.sys.panic(msg) -> never
// baml.hash.sha256(s: string) -> string                        // content-addressed checkpoint keys
// baml.list.push(xs, x) -> xs'                                 // pure list append
// baml.llm.render_messages / openai_response_format / forced_schema_tool
// baml.sap.parse<T>(s: string) -> T
// baml.json.{object,array,string,int,encode} ; baml.str.{concat,eq} ; baml.http.{bearer,anthropic_headers,send}
