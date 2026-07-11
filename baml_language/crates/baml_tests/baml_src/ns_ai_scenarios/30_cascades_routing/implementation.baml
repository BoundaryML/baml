// 30 — quality cascades and business routing are ordinary BAML control flow.
class s30_CascadeOutcome {
  answer: string,
  tier: string,
  escalated: bool,
}

// Task sketch: s30_Answer(question) -> string.

// Hand-written reference expansion of the compiler task surface for
// `s30_Answer`; these helper names are scaffolding, not literal generated names:
// - `s30_Answer_request` represents `s30_Answer.request(...)`.
// - `s30_Answer_manual` spells the plain `s30_Answer(...)` lowering through `root.ai.run`.
function s30_Answer_request(
  question: string,
  provider: root.ai.Provider = root.ai.FakeGenerateProvider { name: "answer", reply: `"fake"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s30_Answer", "ns_ai_scenarios", { "question": question },
    [], root.ai.RequestOptions {}, {}, root.ai.prompt_template(`Answer in one short line: ${question}`, false),
  )
}

function s30_Answer_manual(
  question: string,
  provider: root.ai.Provider = root.ai.FakeGenerateProvider { name: "answer", reply: `"fake"`, attributes: {} },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s30_Answer_request(question, provider = provider))
}

function s30_gated_cascade(
  cheap: root.ai.Provider,
  strong: root.ai.Provider,
  question: string,
  must_include: string,
) -> s30_CascadeOutcome throws baml.errors.CallError | baml.errors.UnknownError {
  let result = root.ai.cascade<string, baml.errors.CallError | baml.errors.UnknownError, baml.errors.CallError | baml.errors.UnknownError>(
    () -> string { s30_Answer_manual(question, provider = cheap) },
    () -> string { s30_Answer_manual(question, provider = strong) },
    (answer: string) -> bool { answer.to_lower_case().includes(must_include.to_lower_case()) },
  );
  s30_CascadeOutcome {
    answer: result.value,
    tier: if (result.selected == 0) { "cheap" } else { "strong" },
    escalated: result.selected != 0,
  }
}

function s30_route(hard: bool, cheap: root.ai.Provider, strong: root.ai.Provider) -> root.ai.Provider throws never {
  if (hard) { strong } else { cheap }
}
