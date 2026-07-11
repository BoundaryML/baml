// 03 — a custom constrained-decoding capability consumes the same Request<T>.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s03_Classify(text: string) -> s03_Sentiment {
//   client: s03_FakeConstrainedProvider { reply: `"Positive"` }
//   prompt: `Classify ${text} as Positive or Negative. ${ctx.output_format}`
// }

// Manual expansion reference for s03_Classify.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s03_Classify_request(...) models s03_Classify.request(...): it renders and
//   packages Request<s03_Sentiment> without performing I/O.
// - s03_Classify_manual(...) models the plain s03_Classify(...) lowering:
//   root.ai.run(request).
// - s03_Classify_constrained_manual(...) is a custom constrained-capability
//   driver over s03_Classify.request(...), not a compiler-provided task modifier.
function s03_Classify_request(
  text: string,
  provider: root.ai.Provider = s03_FakeConstrainedProvider { reply: `"Positive"` },
) -> root.ai.Request<s03_Sentiment> {
  root.ai.task_request<s03_Sentiment>(
    provider,
    "s03_Classify",
    "ns_ai_scenarios",
    { "text": text },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Classify ${text} as Positive or Negative.`, true),
  )
}

function s03_Classify_manual(
  text: string,
  provider: root.ai.Provider = s03_FakeConstrainedProvider { reply: `"Positive"` },
) -> s03_Sentiment throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s03_Sentiment>(s03_Classify_request(text, provider = provider))
}

function s03_Classify_constrained_manual(
  text: string,
  provider: root.ai.Provider = s03_FakeConstrainedProvider { reply: `"Positive"` },
) -> s03_Sentiment throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run_constrained<s03_Sentiment>(
    s03_Classify_request(text, provider = provider),
    root.ai.ChoiceConstraint { values: ["Positive", "Negative"] },
  )
}

test "03: the baseline manual driver remains available" {
  assert.equal(s03_Classify_manual("wonderful"), s03_Sentiment.Positive)
}

test "03: the custom driver negotiates the capability at runtime" {
  assert.equal(s03_Classify_constrained_manual("wonderful"), s03_Sentiment.Positive)
}

test "03: a plain Generate-only provider is a typed Unsupported" {
  let status = s03_Classify_constrained_manual(
    "wonderful",
    provider = root.ai.FakeGenerateProvider { name: "plain", reply: `"Positive"`, attributes: {} },
  ) catch (e) {
    let ignored: baml.errors.Unsupported => "unsupported",
    _ => "wrong-error",
  };
  assert.equal(status, "unsupported")
}
