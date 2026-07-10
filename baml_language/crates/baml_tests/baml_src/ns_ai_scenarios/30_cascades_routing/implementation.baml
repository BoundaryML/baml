// 30 — quality cascades and business routing are ordinary BAML control flow.
class s30_CascadeOutcome {
  answer: string,
  tier: string,
  escalated: bool,
}

// Task sketch: s30_Answer(question) -> string.

// This is the desugared code for s30_Answer ---------------------------------
function s30_Answer_request(
  question: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "answer", reply: `"fixture"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s30_Answer", "ns_ai_scenarios", { "question": question },
    [], root.ai.RequestOptions {}, {}, bepv2_prompt_template(`Answer in one short line: ${question}`, false),
  )
}

function s30_Answer_manual(
  question: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "answer", reply: `"fixture"`, attributes: {} },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s30_Answer_request(question, provider = provider))
}

function s30_gated_cascade(
  cheap: root.ai.Provider,
  strong: root.ai.Provider,
  question: string,
  must_include: string,
) -> s30_CascadeOutcome throws baml.errors.CallError | baml.errors.UnknownError {
  let first = s30_Answer_manual(question, provider = cheap);
  if (first.to_lower_case().includes(must_include.to_lower_case())) {
    return s30_CascadeOutcome { answer: first, tier: "cheap", escalated: false };
  }
  s30_CascadeOutcome { answer: s30_Answer_manual(question, provider = strong), tier: "strong", escalated: true }
}

function s30_route(hard: bool, cheap: root.ai.Provider, strong: root.ai.Provider) -> root.ai.Provider throws never {
  if (hard) { strong } else { cheap }
}
