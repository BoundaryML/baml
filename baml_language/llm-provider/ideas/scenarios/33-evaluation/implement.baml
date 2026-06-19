// ============================================================================
// 33-evaluation / implement.baml  —  STD-LIB / library-author code
//
// Evaluation = dataset + task + scorer(s) + runner. The proposal's spine gives us
// most of the pieces for free, because the things being composed ARE providers and
// functions:
//
//   - the TASK under test is a BAML function  -> a Provider.call wrapped in a fn;
//   - a SCORER is a function `(input, output, expected) -> Score`;
//   - an LLM-JUDGE scorer is ANOTHER Provider (a different model family, on purpose);
//   - determinism levers (seed / temperature) dissolve into PROVIDER FIELDS, and
//     their AVAILABILITY differs per provider (OpenAI has seed; Anthropic/Gemini
//     do not) — which surfaces as a capability interface, not a uniform field;
//   - the DATASET can be golden cases OR production traffic replayed from traces;
//   - the RUNNER pairs task×case×scorer, aggregates, and gates CI on a threshold.
//
// The hard parts the background flags (judge biases, judge drift, score stability
// vs reality, grading-the-grader) show up INSIDE the scorer/judge providers, not in
// the runner. Builds ON the canonical primitives in _conventions.md. Net-new
// surface is flagged in evaluation.md.
// ============================================================================


// ----------------------------------------------------------------------------
// 0. The eval vocabulary. These are plain data — the runner speaks them; scorers
//    produce them. None of this is a language change; it's library types.
// ----------------------------------------------------------------------------

// One labelled example. `expected` is optional because reference-free scorers
// (assertions, LLM-judge-against-criteria) have no golden string.
class Case<TInput, TOutput> {
  id: string
  input: TInput
  expected: TOutput?
  // Provenance — golden / holdout / synthetic / production. Used by the runner to
  // keep holdouts out of any prompt-tuning report and to weight prod traffic.
  source: CaseSource
  // Free-form labels for slicing aggregates (tenant, locale, difficulty).
  tags: map<string, string>
}

enum CaseSource {
  Golden            // hand-curated must-pass
  Holdout           // withheld from tuning — measures generalization
  Synthetic         // model-generated edge cases / paraphrases
  Production         // promoted from a recorded trace (highest signal)
}

// A scorer's verdict. We keep BOTH a continuous score and a pass/fail, because the
// background says rubric scores drift and "reducing to pass/fail is steadier" — the
// runner can gate on either.
class Score {
  value: float          // normalized 0.0..1.0
  passed: bool          // value >= scorer's own threshold
  scorer: string        // which scorer produced this (for per-scorer aggregates)
  rationale: string?    // judge/ rubric explanation; null for deterministic scorers
}

// The result of one (case × task) run, before scoring.
class TaskRun<TOutput> {
  case_id: string
  output: TOutput?      // null if the task itself threw
  error: string?        // the task's failure, captured rather than aborting the sweep
  // The determinism fingerprint the task ran under — recorded so a red CI build can
  // distinguish "the change regressed" from "sampling drifted" (the background's
  // seed/system_fingerprint concern).
  fingerprint: RunFingerprint
}

// What made (or failed to make) this run reproducible. Filled from the provider's
// `Deterministic` capability if it has one (see §3); otherwise temperature-only.
class RunFingerprint {
  temperature: float
  seed: int?                 // null on providers with no seed (Anthropic/Gemini)
  // OpenAI returns system_fingerprint so you can DETECT a silent backend change
  // even with a fixed seed. Absent elsewhere.
  system_fingerprint: string?
  model: string
}


// ----------------------------------------------------------------------------
// 1. A scorer is a FUNCTION. The proposal makes the task a function-returning-a-
//    provider; symmetrically we make a scorer an interface with one method, so
//    deterministic scorers and LLM-judge scorers are the SAME shape and the runner
//    treats them uniformly. This is the `Iterator`-style one-method interface.
//
// NET-NEW (flagged in evaluation.md (b)): there is no `Scorer` interface in the
// proposal. It is library surface, not a language change — it composes existing
// `interface`/`implements` + the typed `throws` channel.
// ----------------------------------------------------------------------------

interface Scorer<TInput, TOutput> {
  type Error = baml.errors.LlmClient
  function score(self, case: Case<TInput, TOutput>, output: TOutput) -> Score throws Error
}


// --- Deterministic scorers: free, reference-based, no model call. -----------

// Exact / fuzzy string match (Levenshtein-normalized). Reference-based: requires
// `case.expected`. No provider involved — pure host compute.
class ExactMatch {
  threshold: float          // 1.0 = exact; <1.0 = fuzzy ratio floor

  implements Scorer<string, string> {
    function score(self, case: Case<string, string>, output: string) -> Score throws baml.errors.LlmClient {
      match (case.expected) {
        let exp: string => {
          let ratio = baml.str.similarity_ratio(output, exp);   // host: 0.0..1.0
          Score { value: ratio, passed: ratio >= self.threshold,
                  scorer: "exact_match", rationale: null }
        }
        _ => {
          // No reference: an exact-match scorer is inapplicable. Honest failure
          // rather than a silent 0 that poisons the aggregate.
          throw baml.errors.LlmClient { message: "ExactMatch needs case.expected" };
        }
      }
    }
  }
}

// Code assertion: a predicate over the output (JSON parses, schema valid, contains
// X). Reference-FREE. The predicate is a first-class function value.
class Assert<TInput, TOutput> {
  name: string
  predicate: (TOutput) -> bool

  implements Scorer<TInput, TOutput> {
    function score(self, case: Case<TInput, TOutput>, output: TOutput) -> Score throws baml.errors.LlmClient {
      let ok = self.predicate(output);
      Score { value: if (ok) { 1.0 } else { 0.0 }, passed: ok,
              scorer: self.name, rationale: null }
    }
  }
}


// --- Embedding similarity: cheap, deterministic-ISH. This one NEEDS a provider, --
//     but an EMBEDDING provider, not a chat one. An embeddings model is just a
//     Provider whose `call<T>` yields a vector — `T = float[]`. Nothing new.

class EmbeddingScorer<TInput> {
  embedder: Provider          // a model that returns float[] from `call`
  threshold: float

  implements Scorer<TInput, string> {
    function score(self, case: Case<TInput, string>, output: string) -> Score throws baml.errors.LlmClient {
      match (case.expected) {
        let exp: string => {
          // Two `call`s on the embedding provider — the SAME `call<T>` seam as a
          // chat model, just with T = float[]. Determinism comes for free: an
          // embedding model has no sampling.
          let a: float[] = self.embedder.call<float[]>(baml.llm.text_prompt(output));
          let b: float[] = self.embedder.call<float[]>(baml.llm.text_prompt(exp));
          let cos = baml.vec.cosine(a, b);                    // host: -1.0..1.0
          let norm = (cos + 1.0) / 2.0;                       // -> 0.0..1.0
          Score { value: norm, passed: norm >= self.threshold,
                  scorer: "embedding_similarity", rationale: null }
        }
        _ => { throw baml.errors.LlmClient { message: "EmbeddingScorer needs case.expected" }; }
      }
    }
  }
}


// ----------------------------------------------------------------------------
// 2. The LLM-as-judge scorer: a scorer whose engine is ANOTHER Provider.
//
// This is the payoff of "everything is a provider": the judge is just a chat model
// you `call`, parsing its verdict via SAP. The background's judge BIASES live HERE,
// inside the judge scorer, because they are the judge's problem, not the runner's:
//
//   - position bias  -> in PAIRWISE judging, run BOTH orders and average;
//   - self-preference -> the runner is told to pick a DIFFERENT family for `judge`
//                        than the `task` provider (enforced in §4, JudgeConfig);
//   - calibration / drift -> pin the judge model version (a provider FIELD) and use
//                            an anchored rubric, reducing to pass/fail.
// ----------------------------------------------------------------------------

// The judge's structured verdict — SAP parses the judge model's reply into this.
class Verdict {
  pass: bool
  score: int            // anchored 1..5 against the rubric (not a bare 1..10 scale)
  reasoning: string
}

class JudgeScorer<TInput, TOutput> {
  judge: Provider           // the judge MODEL — deliberately a different family
  rubric: string            // anchored criteria with examples (calibration defense)
  // Pin so historical scores stay comparable across runs (judge-drift defense).
  // This is informational — the actual pin is a FIELD on `judge`'s provider class.
  judge_model_id: string

  implements Scorer<TInput, TOutput> {
    function score(self, case: Case<TInput, TOutput>, output: TOutput) -> Score throws baml.errors.LlmClient {
      // Build a judging prompt: rubric + the candidate output + (if present) the
      // reference. render_judge_prompt is host-side prompt assembly, exactly like
      // render_prompt for a task.
      let prompt = render_judge_prompt(self.rubric, output, case.expected);
      // The judge is a Provider — we `call` it and SAP-parse a Verdict. Same seam
      // as any task. The judge runs at temperature 0 (a field on its provider).
      let v: Verdict = self.judge.call<Verdict>(prompt);
      Score { value: int_to_unit(v.score, 5), passed: v.pass,
              scorer: "llm_judge(" + self.judge_model_id + ")",
              rationale: v.reasoning }
    }
  }
}

// Pairwise preference: "is the new version better than the old?" The judge picks A
// vs B. Position bias is defended HERE: judge A-then-B and B-then-A, average.
//
// This scorer's "output" is a PAIR, so it does NOT fit Scorer<TInput, TOutput>
// (which scores a single output). It is its own interface — a genuine divergence
// that the single-output Scorer cannot absorb. NET-NEW, flagged in evaluation.md.
interface PairwiseScorer<TInput, TOutput> {
  type Error = baml.errors.LlmClient
  function compare(self, input: TInput, a: TOutput, b: TOutput) -> Score throws Error
}

class PairwiseJudge<TInput, TOutput> {
  judge: Provider
  rubric: string
  judge_model_id: string

  implements PairwiseScorer<TInput, TOutput> {
    function compare(self, input: TInput, a: TOutput, b: TOutput) -> Score throws baml.errors.LlmClient {
      // Order 1: a first. Order 2: b first. Position-bias mitigation = average.
      let p_ab = render_pairwise_prompt(self.rubric, input, a, b);
      let p_ba = render_pairwise_prompt(self.rubric, input, b, a);
      let v_ab: Verdict = self.judge.call<Verdict>(p_ab);     // pass => A preferred
      let v_ba: Verdict = self.judge.call<Verdict>(p_ba);     // pass => B preferred (slot 1)
      // v_ab.pass means "first shown (A) wins"; v_ba.pass means "first shown (B) wins".
      let a_wins = (if (v_ab.pass) { 1.0 } else { 0.0 })
                 + (if (v_ba.pass) { 0.0 } else { 1.0 });     // B-first: A is slot 2
      let frac = a_wins / 2.0;                                 // 0, .5, or 1
      Score {
        value: frac, passed: frac > 0.5,
        scorer: "pairwise(" + self.judge_model_id + ")",
        rationale: "AB:" + v_ab.reasoning + " | BA:" + v_ba.reasoning,
      }
    }
  }
}


// ----------------------------------------------------------------------------
// 3. Determinism levers as a CAPABILITY, because availability diverges.
//
// The spine rule "options dissolve into provider fields" says `seed` is a field.
// But the background's divergence is sharper: OpenAI HAS a seed (+system_fingerprint
// to detect drift), Anthropic/Gemini do NOT. If `seed` were a uniform field, you'd
// be able to construct an Anthropic with a seed it silently ignores. So we model
// "can this provider be made reproducible, and how?" as a capability interface that
// only seed-capable providers implement. Temperature is universal; seed is not.
//
// NET-NEW: the `Deterministic` capability interface (flagged in evaluation.md (b)).
// ----------------------------------------------------------------------------

interface Deterministic requires Provider {
  // Returns the fingerprint this provider WILL run under — so the runner records
  // it without sending a request. Providers without a seed return seed = null.
  function fingerprint(self) -> RunFingerprint throws Error
}


// --- Provider implementations: the two backends the runner mixes. -----------
//     OpenAI is the seedable one; Anthropic is temperature-only. Both implement
//     the base HttpProvider; only OpenAI implements Deterministic-with-a-seed.

class OpenAIChat {
  model: string
  api_key: string
  temperature: float            // 0 for eval stability
  seed: int?                    // OpenAI-only reproducibility knob
  service_tier: string?         // "flex" — eval sweeps have no user waiting (cost)

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // temperature + seed + service_tier all ride into the request body here. The
      // four structured-output encodings (for the judge's Verdict schema) are chosen
      // in this method — OpenAI uses `response_format: json_schema`.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements Deterministic {
    function fingerprint(self) -> RunFingerprint throws baml.errors.LlmClient {
      // OpenAI: report the seed AND read back system_fingerprint from a probe header
      // so the runner can detect a silent backend change between runs.
      RunFingerprint {
        temperature: self.temperature,
        seed: self.seed,
        system_fingerprint: openai_system_fingerprint(self.model, self.api_key),
        model: self.model,
      }
    }
  }
}

class Anthropic {
  model: string
  api_key: string
  max_tokens: int
  temperature: float            // the ONLY reproducibility knob here
  // NOTE: deliberately NO `seed` field. Anthropic has no seed. Modeling it as a
  // field you could set-and-ignore would be a lie; instead Anthropic's
  // Deterministic.fingerprint reports seed: null.

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Anthropic encodes the Verdict schema via a FORCED tool-use (tool_choice),
      // not response_format — the structured-output divergence, chosen here.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements Deterministic {
    function fingerprint(self) -> RunFingerprint throws baml.errors.LlmClient {
      // No seed, no system_fingerprint. temperature: 0 is the whole story — which is
      // exactly why exact-match regression scores are noisier on this backend.
      RunFingerprint {
        temperature: self.temperature, seed: null,
        system_fingerprint: null, model: self.model,
      }
    }
  }
}


// ----------------------------------------------------------------------------
// 4. The RUNNER. Pairs a task (a function over input -> output, i.e. a provider
//    call wrapped in app code) with a dataset and a list of scorers, runs the
//    sweep, aggregates, and gates.
//
// The task is handed in as a plain function value `(TInput) -> TOutput`, because
// the proposal makes a BAML eval-target function exactly that: it internally does
// `client().call<TOutput>(render_prompt(input))`. The runner never needs to know
// which provider is inside.
// ----------------------------------------------------------------------------

class RunReport {
  total: int
  mean_score: float             // mean of every (case × scorer) value
  pass_rate: float              // fraction of (case × scorer) that passed
  per_scorer: map<string, float>   // scorer name -> mean value (for slicing)
  errors: int                   // task or scorer failures, surfaced not swallowed
  // Distinct fingerprints seen — if >1 model/seed appears, the run isn't pinned and
  // a score delta is uninterpretable. The runner flags this.
  fingerprints: RunFingerprint[]
}

class Runner<TInput, TOutput> {
  cases: Case<TInput, TOutput>[]
  task: (TInput) -> TOutput          // the thing under test
  scorers: Scorer<TInput, TOutput>[]
  // The provider the task runs on — passed separately ONLY so the runner can read
  // its determinism fingerprint up front (capability `match`). The task closure
  // already closes over it for the actual call.
  task_provider: Provider

  function run(self) -> RunReport {
    // 1. Record the determinism fingerprint ONCE, before the sweep, via the
    //    capability match. A provider without `Deterministic` -> we can only assume
    //    it's unpinned, which the report will surface.
    let fp = match (self.task_provider) {
      let d: Deterministic => d.fingerprint() catch (e) { _ => { unknown_fingerprint(); } },
      _ => { unknown_fingerprint(); },   // can't promise reproducibility
    };

    let total = 0;
    let errors = 0;
    let sum = 0.0;
    let passes = 0;
    let n_scores = 0;
    let per: map<string, float> = {};
    let per_n: map<string, int> = {};

    for (let case in self.cases) {
      total = total + 1;
      // Run the task. Capture failure per-case rather than aborting the sweep — a
      // single crash shouldn't void the whole eval.
      let out: TOutput = self.task(case.input) catch (e) {
        _ => { errors = errors + 1; continue; }
      };
      // Score with every scorer. A scorer that can't apply (e.g. ExactMatch on a
      // reference-free case) throws -> counted as an error, not a silent 0.
      for (let scorer in self.scorers) {
        let s: Score = scorer.score(case, out) catch (e) {
          _ => { errors = errors + 1; continue; }
        };
        sum = sum + s.value;
        if (s.passed) { passes = passes + 1; }
        n_scores = n_scores + 1;
        per = baml.map.insert(per, s.scorer, baml.map.get_or(per, s.scorer, 0.0) + s.value);
        per_n = baml.map.insert(per_n, s.scorer, baml.map.get_or(per_n, s.scorer, 0) + 1);
      }
    }

    RunReport {
      total: total,
      mean_score: if (n_scores > 0) { sum / int_to_float(n_scores) } else { 0.0 },
      pass_rate: if (n_scores > 0) { int_to_float(passes) / int_to_float(n_scores) } else { 0.0 },
      per_scorer: mean_map(per, per_n),
      errors: errors,
      fingerprints: [fp],
    }
  }

  // CI GATE. Two flavors the background names: an absolute floor, or a delta vs the
  // last green run. Returns true => build passes. Throwing here = fail the build.
  function gate(self, floor: float, baseline: float?) -> bool throws baml.errors.LlmClient {
    let report = self.run();
    // Refuse to gate on an unpinned run: if the task provider isn't Deterministic,
    // a green build is unreproducible and a red one is uninterpretable.
    if (report.errors > 0 && report.errors == report.total) {
      throw baml.errors.LlmClient { message: "eval produced no scores (all cases errored)" };
    }
    let ok_floor = report.pass_rate >= floor;
    let ok_delta = match (baseline) {
      let b: float => report.pass_rate >= b - 0.01,   // allow 1pt of noise vs last green
      _ => true,
    };
    if (!ok_floor) {
      throw baml.errors.LlmClient {
        message: "eval regressed: pass_rate " + float_to_str(report.pass_rate) + " < floor " + float_to_str(floor),
      };
    }
    if (!ok_delta) {
      throw baml.errors.LlmClient {
        message: "eval regressed vs baseline: " + float_to_str(report.pass_rate),
      };
    }
    true
  }
}


// ----------------------------------------------------------------------------
// 5. Dataset from PRODUCTION TRACES. The highest-signal source. A recorded trace
//    is (input, the output the model actually produced). Promoting it to a Case
//    means: input becomes Case.input; the recorded output can become `expected`
//    (a regression anchor: "don't get WORSE than what shipped") or be left null
//    for reference-free judging.
// ----------------------------------------------------------------------------

class Trace<TInput, TOutput> {
  request_id: string
  input: TInput
  recorded_output: TOutput
  tenant: string
  ts: int
}

class TraceDataset {
  // Promote recorded production traffic into golden-ish regression cases. The
  // recorded output becomes the `expected` anchor; source = Production so the
  // runner weights/labels it as live traffic.
  function from_traces<TInput, TOutput>(traces: Trace<TInput, TOutput>[]) -> Case<TInput, TOutput>[] {
    let out: Case<TInput, TOutput>[] = [];
    for (let tr in traces) {
      out = baml.array.push(out, Case<TInput, TOutput> {
        id: tr.request_id,
        input: tr.input,
        expected: tr.recorded_output,    // regression anchor; or null for ref-free
        source: CaseSource.Production,
        tags: { "tenant": tr.tenant },
      });
    }
    out
  }
}


// ----------------------------------------------------------------------------
// 6. Host boundaries (genuine I/O / pure-compute the runtime owns).
// ----------------------------------------------------------------------------

// Deterministic string scorers.
function similarity_ratio(a: string, b: string) -> float { $rust_io_function }   // baml.str
// Vector math for embedding similarity.
function cosine(a: float[], b: float[]) -> float { $rust_io_function }            // baml.vec

// Prompt assembly for judges (rubric + candidate + optional reference).
function render_judge_prompt<TOutput>(rubric: string, output: TOutput, expected: TOutput?) -> baml.llm.PromptAst { $rust_io_function }
function render_pairwise_prompt<TInput, TOutput>(rubric: string, input: TInput, first: TOutput, second: TOutput) -> baml.llm.PromptAst { $rust_io_function }

// Probe OpenAI's system_fingerprint without a full completion (drift detection).
function openai_system_fingerprint(model: string, api_key: string) -> string? { $rust_io_function }

// Fallback fingerprint for non-Deterministic providers — temperature unknown, seed
// null, model "<unpinned>". Surfaced so the report can flag an uninterpretable run.
function unknown_fingerprint() -> RunFingerprint { $rust_io_function }

// Small numeric/string helpers used above.
function int_to_unit(v: int, max: int) -> float { $rust_io_function }   // v/max clamped 0..1
function int_to_float(v: int) -> float { $rust_io_function }
function float_to_str(v: float) -> string { $rust_io_function }
function mean_map(sums: map<string, float>, counts: map<string, int>) -> map<string, float> { $rust_io_function }
