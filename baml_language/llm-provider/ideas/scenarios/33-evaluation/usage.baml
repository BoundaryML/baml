// ============================================================================
// 33-evaluation / usage.baml  —  APPLICATION-author code
//
// What an app author writes to evaluate a real BAML pipeline: a task function, a
// dataset (golden + promoted-from-prod), a mix of scorers (deterministic gate +
// LLM-judge for the open-ended part), and a CI gate. The point: the task and the
// judge are BOTH just providers, so the app author writes very little plumbing.
// ============================================================================


// ---------------------------------------------------------------------------
// The task under test: a support-bot reply. This is the production function — the
// SAME function that serves users. `client` is sugar for a function returning a
// Provider, so the eval runs the exact code path production runs.
//
// For eval stability we pin temperature 0 and a seed. OpenAI takes the seed;
// Anthropic (below) ignores it because it has no `seed` field at all.
// ---------------------------------------------------------------------------

client SupportModel() {
  OpenAIChat {
    model: "gpt-4.1",
    api_key: env.OPENAI_API_KEY,
    temperature: 0.0,
    seed: 7,                       // OpenAI-only reproducibility knob
    service_tier: "flex",          // eval sweep: no user waiting -> cheaper/slower
  }
}

function SupportReply(question: string) -> string {
  client: SupportModel()
  prompt #"You are a concise support agent. Answer: {{ question }}"#
}


// ---------------------------------------------------------------------------
// The JUDGE model. DELIBERATELY a different family from the task (Anthropic vs
// OpenAI) to dodge self-preference bias — the background's mitigation. It runs at
// temperature 0 and a pinned model id so historical scores stay comparable
// (judge-drift defense). It has no `seed` field; that's correct, Anthropic has none.
// ---------------------------------------------------------------------------

client JudgeModel() {
  Anthropic {
    model: "claude-sonnet-4-6-20260514",   // PINNED version — drift defense
    api_key: env.ANTHROPIC_API_KEY,
    max_tokens: 1024,
    temperature: 0.0,
  }
}

// An embedding model for semantic scoring. Also "just a provider"; T = float[].
client EmbedModel() {
  OpenAIChat {
    model: "text-embedding-3-large",
    api_key: env.OPENAI_API_KEY,
    temperature: 0.0,              // unused by embeddings, but the field exists
  }
}


// ---------------------------------------------------------------------------
// The scorer stack. Cheap deterministic scorers gate the bulk; the LLM-judge
// handles the open-ended "is this a good support answer?" where no golden string
// exists. Each scorer is a value the app constructs — no framework registration.
// ---------------------------------------------------------------------------

function support_scorers() -> Scorer<string, string>[] {
  [
    // 1. Cheap, deterministic: the answer must mention the refund window if the
    //    question is about refunds. A code assertion — reference-free.
    Assert<string, string> {
      name: "mentions_policy",
      predicate: (out) => baml.str.contains(out, "30 day"),
    },
    // 2. Embedding similarity to the golden answer where one exists.
    EmbeddingScorer<string> {
      embedder: EmbedModel(),       // the embedding provider, by value
      threshold: 0.82,
    },
    // 3. LLM-as-judge against an anchored rubric. The judge is a DIFFERENT family.
    JudgeScorer<string, string> {
      judge: JudgeModel(),
      judge_model_id: "claude-sonnet-4-6-20260514",
      rubric: #"
        Score the support reply 1-5 against these anchors:
        5 = correct, complete, polite, no hallucinated policy.
        3 = correct but missing a relevant detail OR slightly off tone.
        1 = wrong, rude, or invents policy that does not exist.
        Pass iff score >= 4.
      "#,
    },
  ]
}


// ---------------------------------------------------------------------------
// The dataset: golden cases hand-written, PLUS production traffic promoted from
// recorded traces. The recorded prod output becomes the `expected` regression
// anchor ("don't get worse than what shipped").
// ---------------------------------------------------------------------------

function golden_cases() -> Case<string, string>[] {
  [
    Case<string, string> {
      id: "refund-window", source: CaseSource.Golden, tags: { "topic": "refunds" },
      input: "How long do I have to return an item?",
      expected: "You have a 30 day return window from delivery.",
    },
    Case<string, string> {
      id: "hours", source: CaseSource.Holdout, tags: { "topic": "hours" },
      input: "When is support open?",
      expected: "Support is available 24/7.",
    },
  ]
}

// Pull last week's real traffic out of the trace store and promote it. These are
// the highest-signal cases — what users actually sent.
function prod_cases() -> Case<string, string>[] {
  let traces: Trace<string, string>[] = load_recorded_traces("support-bot", 7);
  TraceDataset.from_traces<string, string>(traces)
}

function support_dataset() -> Case<string, string>[] {
  baml.array.concat(golden_cases(), prod_cases())
}


// ---------------------------------------------------------------------------
// The runner + CI gate. This is what CI invokes on every prompt/model change.
// One construction, one `.gate(...)` call; a regression THROWS and fails the build.
// ---------------------------------------------------------------------------

function eval_support() -> RunReport {
  let runner = Runner<string, string> {
    cases: support_dataset(),
    task: (q) => SupportReply(q),          // the production function, as a value
    scorers: support_scorers(),
    task_provider: SupportModel(),          // so the runner can pin a fingerprint
  };
  runner.run()
}

// CI entrypoint: floor of 95% pass-rate, OR no worse than the last green run.
// Throwing => non-zero exit => red build.
function ci_gate_support(baseline_pass_rate: float?) -> bool throws baml.errors.LlmClient {
  let runner = Runner<string, string> {
    cases: support_dataset(),
    task: (q) => SupportReply(q),
    scorers: support_scorers(),
    task_provider: SupportModel(),
  };
  runner.gate(0.95, baseline_pass_rate)
}


// ---------------------------------------------------------------------------
// "Is the NEW prompt better than the OLD one?" — pairwise / eval-driven dev. Run
// both task variants over the dataset and let a pairwise judge pick. Position bias
// is handled inside PairwiseJudge (both orders, averaged).
// ---------------------------------------------------------------------------

client SupportModelV2() {
  OpenAIChat {
    model: "gpt-4.1", api_key: env.OPENAI_API_KEY,
    temperature: 0.0, seed: 7, service_tier: "flex",
  }
}

function SupportReplyV2(question: string) -> string {
  client: SupportModelV2()
  prompt #"You are a warm, concise support agent. Be specific about policy. Answer: {{ question }}"#
}

function compare_prompts() -> float {
  let judge = PairwiseJudge<string, string> {
    judge: JudgeModel(),
    judge_model_id: "claude-sonnet-4-6-20260514",
    rubric: "Prefer the reply that is more correct, complete, and on-tone.",
  };
  let cases = support_dataset();
  let v2_wins = 0.0;
  let n = 0;
  for (let case in cases) {
    let a: string = SupportReply(case.input)   catch (e) { _ => { continue; } };
    let b: string = SupportReplyV2(case.input) catch (e) { _ => { continue; } };
    // compare returns A-preference; we want B (=v2) preference, so invert.
    let s: Score = judge.compare(case.input, a, b) catch (e) { _ => { continue; } };
    if (!s.passed) { v2_wins = v2_wins + 1.0; }   // A not preferred => B(=v2) won
    n = n + 1;
  }
  if (n > 0) { v2_wins / int_to_float(n) } else { 0.0 }
}


// ---------------------------------------------------------------------------
// Host boundary used only by usage code.
// ---------------------------------------------------------------------------

// Pull recorded production traffic from the trace store (Observability scenario).
function load_recorded_traces(function_name: string, days: int) -> Trace<string, string>[] { $rust_io_function }
