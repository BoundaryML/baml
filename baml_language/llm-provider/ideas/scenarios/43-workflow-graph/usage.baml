// ============================================================================
// usage.baml — APPLICATION-author code: a durable doc-processing step graph.
//
// This is the Mastra `doc-pipeline` from background §2, written as plain BAML.
// The whole point: there is NO workflow DSL. The graph is the function body.
//
//   fetch ──> [summarize ‖ classify]  ──join──> branch(label) ──> dountil(refine)
//             (the SYNC BARRIER fan-in is two spawns joined with baml.future.all)
//
// Demonstrates:
//   - a step = a typed BAML `function`; an edge = `let b = StepB(StepA(x))`
//   - a model call inside a step = `provider.call<T>(...)` — nothing special
//   - fan-out + JOIN via spawn + baml.future.all (the sync barrier — a shipped
//     language feature, BEP-034; not net-new)
//   - branch = `match`; loop = `while` with an explicit iteration cap
//   - durability via `step(ckpt, ...)`: a crashed run resumes from the middle
//   - TWO divergent providers (OpenAI response_format vs Anthropic forced-tool)
//     used in different steps with identical call-sites — portability proof
//   - a node that is itself a MODEL-DECIDES loop (.run_tools) — code-defines and
//     model-decides composing in one graph
// ============================================================================


// ---- config-only clients (sugar for `function Name() -> Provider`) ----------

client Summarizer()  { OpenAI.gpt("gpt-4o") }          // response_format encoding
client Classifier()  { Anthropic.claude("claude-haiku-4") }   // forced-tool encoding
client Refiner()     { OpenAI.gpt("gpt-4o") }
client LegalAgent()  { Anthropic.claude("claude-opus-4-1") }  // the tool-using node


// ---- the schemas that flow across the edges (background §what's hard:
//      "guaranteeing step B's input is satisfied by step A's output") ----------

class Doc        { text: string, url: string }
class Summary    { summary: string }
class Label      { label: string, confidence: float }   // "legal" | "spam" | "ok"
class Refined    { summary: string, score: float }
class Reviewed   { verdict: string, citations: string[] }

// The fan-in JOIN shape. This is the keyed object Mastra's `.parallel` produces
// and the reducer that pydantic-graph/LangGraph fold into. In BAML it is just a
// class — we name the two branch outputs and merge by construction. No reducer
// machinery, no clobbering: the join is a plain record literal.
class Analyzed {
  summary: Summary
  label: Label
}

// The terminal output of the whole graph.
class PipelineResult {
  refined: Refined
  review: Reviewed?      // present only if the legal branch ran
  label: string
}

// A graph-orchestration domain failure: the dountil loop exhausted its cap. It is
// neither a transport failure nor a refusal, but it travels on the node's CallError
// channel so the app can catch it concretely (most-specific-first) alongside real
// provider errors. None of the transport classifiers apply.
class RefineNotConverged {
  iterations: int
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}


// ============================================================================
// (A) The steps. Each is an ordinary typed function whose body does ONE thing
//     and (where it needs a model) calls a Provider. The "model call inside a
//     step is just Provider.call" claim, made literal.
// ============================================================================

// A non-model step. Fetching a doc is host I/O; no provider involved. Proves a
// step need not touch an LLM at all — the graph mixes model and non-model nodes.
function FetchDoc(url: string) -> Doc throws baml.ExtendUnknownError<baml.errors.CallError> {
  let text = http_get_text(url) catch (e) { _ => { throw baml.UnknownError.with_message<baml.errors.CallError>(e, "fetch failed"); } };
  Doc { text: text, url: url }
}

// A model step. The provider call is the ONLY interesting line. Summarizer() is
// an OpenAI provider, so this goes out as response_format — but this code can't
// tell, and doesn't care.
function Summarize(doc: Doc) -> Summary throws baml.ExtendUnknownError<baml.errors.CallError> {
  let p = Summarizer();
  p.call<Summary>(render_summarize(doc.text))
}

// Another model step — but Classifier() is ANTHROPIC, so the SAME call shape
// emits a forced-tool request and reads the tool input back. Identical call-site;
// divergence absorbed entirely inside the provider. This is the portability proof.
function Classify(doc: Doc) -> Label throws baml.ExtendUnknownError<baml.errors.CallError> {
  let p = Classifier();
  p.call<Label>(render_classify(doc.text))
}

// A loop step body: one refinement pass. The dountil/loop lives in the CALLER,
// not here — this is just "improve once and score". Code-defines, not model-decides.
function RefineOnce(s: Summary, prev_score: float) -> Refined throws baml.ExtendUnknownError<baml.errors.CallError> {
  let p = Refiner();
  p.call<Refined>(render_refine(s.summary, prev_score))
}

// A node that is itself a MODEL-DECIDES loop. The legal review is open-ended:
// the model calls tools (search case law, fetch statute) until it is done. That
// is the Tools capability — `.run_tools` — used as ONE node of the code-defined
// graph. The two paradigms compose with no glue.
function LegalReview(doc: Doc) -> Reviewed throws baml.ExtendUnknownError<baml.errors.ToolError> {
  // .run_tools is the generated Tools companion of this function's client. It
  // match-es LegalAgent() for the Tools capability and runs begin/step/submit.
  LegalReviewFn.run_tools<Reviewed>(
    doc.text,
    [
      Tool { name: "search_caselaw", description: "find relevant precedent", parameters: CaselawQuery },
      Tool { name: "fetch_statute",  description: "fetch a statute by citation", parameters: StatuteRef },
    ],
    ToolRegistryCtx { _registry: legal_tools() },
  )
}

// The function the .run_tools companion is generated from (config-only client).
function LegalReviewFn(text: string) -> Reviewed {
  client: LegalAgent()
  prompt #"Review this document for legal risk, citing precedent: {{ text }}"#
}

class CaselawQuery { query: string }
class StatuteRef   { citation: string }


// ============================================================================
// (B) The GRAPH. This is the entire workflow — a single function whose body is
//     ordinary control flow. Read it top to bottom; that IS the topology.
//
//     Durable: every step is wrapped in `step(ckpt, run_id, ...)`, so a crash
//     mid-graph resumes from the first incomplete step on re-run. The Checkpoint
//     wraps step execution; the providers stay stateless and single-shot.
// ============================================================================

function DocPipeline(url: string, run_id: string, ckpt: Checkpoint) -> PipelineResult throws baml.ExtendUnknownError<baml.errors.CallError> {

  // ---- sequential edge: fetch, then analyze ----
  // `step(...)` memoizes: on a resumed run this returns the cached Doc without
  // re-fetching. The body is a thunk so it only runs on a cache miss.
  let doc: Doc = step<Doc>(ckpt, run_id, "fetch", url, () => { FetchDoc(url) });

  // ---- FAN-OUT + JOIN: the sync barrier ----
  // Mastra's `.parallel([summarize, classify])`. We `spawn` BOTH model calls so
  // they start concurrently, then `await` both — the second await is the sync
  // barrier (it blocks until that future lands; the first already settled or is
  // settling). Each branch is independently checkpointed, so a crash after
  // summarize-but-before-classify resumes with summarize cached.
  //
  // This is plain shipped concurrency (BEP-034): `spawn { }` returns a Future
  // immediately; `await` joins it. No host scheduler seam, no net-new primitive —
  // the language already expresses fan-out as a list of spawns.
  let fSummary: baml.future.Future<Summary, baml.ExtendUnknownError<baml.errors.CallError>> =
    spawn { step<Summary>(ckpt, run_id, "summarize", doc.text, () => { Summarize(doc) }) };
  let fLabel: baml.future.Future<Label, baml.ExtendUnknownError<baml.errors.CallError>> =
    spawn { step<Label>(ckpt, run_id, "classify", doc.text, () => { Classify(doc) }) };
  // Fan-IN: await both and merge into one keyed record. In Mastra this is the
  // `.map(...)` reshape after `.parallel`; here it is a plain literal over the two
  // joined values. No reducer, no key collision — the join is construction.
  let analyzed: Analyzed = Analyzed { summary: await fSummary, label: await fLabel };

  // ---- BRANCH: exactly one path runs (Mastra `.branch`, pydantic return-union) ----
  // The decision lives in a `match`, not a separate combinator. Each arm must
  // produce the SAME downstream shape (Reviewed?) so the join below is uniform —
  // exactly the "all branch steps share output schema" constraint from background.
  let review: Reviewed? = match (analyzed.label.label) {
    "legal" => {
      // The model-decides node: an agentic tool loop as one graph step. NOTE the
      // channel seam: `step` runs on the CallError channel (the node-body default),
      // but LegalReview throws on the ToolError channel. The two capability error
      // channels do not unify, so the thunk normalizes the ToolError into the
      // node's channel via UnknownError — a typed ToolError is flattened to an
      // opaque UnknownError the moment it crosses a differently-typed step seam.
      step<Reviewed>(ckpt, run_id, "legal_review", doc.text, () => {
        LegalReview(doc) catch (e) { _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "legal review node failed"); }
      })
    }
    "spam"  => { null }     // drop it — no review needed
    _       => { null }     // "ok" path
  };

  // ---- LOOP: dountil(refine, score >= 0.9), with an explicit iteration cap ----
  // Graph frameworks loop by routing an edge back to an earlier node; here it is
  // a `while`. Background §what's hard: "loops have no built-in cap; the idiom is
  // to track an iterationCount and throw past a threshold." We do exactly that —
  // the cap is the app's responsibility, made visible.
  let refined: Refined = step<Refined>(ckpt, run_id, "refine", analyzed.summary.summary, () => {
    let current: Refined = RefineOnce(analyzed.summary, 0.0);
    let iters = 1;
    while (current.score < 0.9) {
      if (iters >= 5) {
        // Loop cap reached — fail the step (and the graph) rather than spin. This
        // is a deliberate orchestration-domain failure with no provider analogue,
        // so it gets a concrete typed CallError the app can branch on.
        throw RefineNotConverged { iterations: iters };
      }
      current = RefineOnce(Summary { summary: current.summary }, current.score);
      iters = iters + 1;
    }
    current
  });

  // ---- terminal join ----
  PipelineResult { refined: refined, review: review, label: analyzed.label.label }
}


// ============================================================================
// (C) Bounded fan-out over an array — Mastra `.foreach(processItem, {concurrency})`.
//     A `baml.spawn.TaskGroup` is the concurrency cap (the rate limiter): every
//     item is spawned into the group, so at most `width` run at once and the rest
//     queue FIFO; `baml.future.all` is the sync-barrier join over them. Each
//     item's step is checkpointed independently, so a batch of 1000 docs that
//     crashes at #700 resumes having to do only the last 300.
// ============================================================================

function BatchPipeline(urls: string[], run_id: string, ckpt: Checkpoint) -> PipelineResult[] throws baml.ExtendUnknownError<baml.errors.CallError> {
  // width=5 fan-out, just like .foreach concurrency:5. The TaskGroup caps live
  // spawns at 5; `baml.future.all` is fail-fast — the first bad doc cancels the
  // remaining futures and re-throws. For all-settled (keep the good ones, let the
  // losers finish), swap `baml.future.all` for `baml.future.all_complete`.
  let g = baml.spawn.TaskGroup.new(5);
  let fs = urls.map((u) => spawn with baml.spawn.options(group = g) {
    DocPipeline(u, run_id + "/" + u, ckpt)
  });
  await baml.future.all(fs)
}


// ============================================================================
// (D) Durability in action: the SAME graph run twice with the same run_id and a
//     KvCheckpoint resumes from the middle. First call crashes after summarize;
//     second call (after redeploy) finds summarize cached and skips it. The
//     graph code is byte-for-byte identical — durability is just the checkpoint
//     store handed in, never a change to a step or a provider.
// ============================================================================

function RunDurable(url: string) -> PipelineResult throws baml.ExtendUnknownError<baml.errors.CallError> {
  let ckpt = KvCheckpoint { namespace: "doc-pipeline", _kv: open_kv("redis://prod") };
  DocPipeline(url, "run-2026-06-18-abc", ckpt)
}

// The test variant: same graph, in-memory checkpoint, no durability needed.
function RunForTest(url: string) -> PipelineResult throws baml.ExtendUnknownError<baml.errors.CallError> {
  DocPipeline(url, "test-run", MemoryCheckpoint.new())
}


// ---- host-side app sinks (the application provides these; extern here) ------
function http_get_text(url: string) -> string throws baml.errors.Io { $rust_io_function }
function render_summarize(text: string) -> baml.llm.PromptAst { $rust_io_function }
function render_classify(text: string) -> baml.llm.PromptAst { $rust_io_function }
function render_refine(summary: string, score: float) -> baml.llm.PromptAst { $rust_io_function }
function legal_tools() -> $rust_type { $rust_io_function }     // name -> host fn registry
function open_kv(dsn: string) -> $rust_type { $rust_io_function }
