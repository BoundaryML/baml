// 33 — evaluation is ordinary code around the same task production calls.
class s33_Verdict {
  pass: bool,
  score: int @description("Integer from 1 (worst) to 5 (best), inclusive."),
  reasoning: string,
}

// Task sketch: s33_SupportReply(question) -> string.

// Hand-written reference expansion of the compiler task surface for
// `s33_SupportReply`; these helper names are scaffolding, not literal generated names:
// - `s33_SupportReply_request` represents `s33_SupportReply.request(...)`.
// - `s33_SupportReply_manual` spells the plain `s33_SupportReply(...)` lowering.
function s33_SupportReply_request(
  question: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "support", reply: `"Please see the refund policy."`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s33_SupportReply", "ns_ai_scenarios", { "question": question },
    [], root.ai.RequestOptions {}, {},
    bepv2_prompt_template(`Answer as a concise support agent. User: ${question}`, false),
  )
}

function s33_SupportReply_manual(
  question: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "support", reply: `"Please see the refund policy."`, attributes: {} },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s33_SupportReply_request(question, provider = provider))
}

// Task sketch: s33_Judge(question, answer) -> s33_Verdict.

// Hand-written reference expansion of the compiler task surface for `s33_Judge`;
// these helper names are scaffolding, not literal generated names:
// - `s33_Judge_request` represents `s33_Judge.request(...)`.
// - `s33_Judge_manual` spells the plain `s33_Judge(...)` lowering.
function s33_Judge_request(
  question: string,
  answer: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "judge", reply: `{"pass":true,"score":5,"reasoning":"fixture"}`, attributes: {} },
) -> root.ai.Request<s33_Verdict> {
  root.ai.task_request<s33_Verdict>(
    provider, "s33_Judge", "ns_ai_scenarios", { "question": question, "answer": answer },
    [], root.ai.RequestOptions {}, {},
    bepv2_prompt_template(
      `Judge the answer. Return score as an integer from 1 to 5 inclusive.\nQuestion: ${question}\nAnswer: ${answer}`,
      true,
    ),
  )
}

function s33_Judge_manual(
  question: string,
  answer: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "judge", reply: `{"pass":true,"score":5,"reasoning":"fixture"}`, attributes: {} },
) -> s33_Verdict throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s33_Verdict>(s33_Judge_request(question, answer, provider = provider))
}

test "33: deterministic task output can be scored locally" {
  let answer = s33_SupportReply_manual("Can I get a refund?");
  assert.contains(answer.to_lower_case(), "refund")
}

test "33: an LLM judge returns a typed verdict" {
  let verdict = s33_Judge_manual("Can I get a refund?", "See the refund policy.");
  assert.is_true(verdict.pass)
  assert.equal(verdict.score, 5)
}

test "33: the judge request states the score scale" {
  let prompt = baml.llm.prompt_to_text(s33_Judge_request("q", "a").prompt);
  assert.contains(prompt, "1 to 5 inclusive")
}

testset "33 eval suite gates on a pass-rate threshold" with testing.PassRate(0.75) {
  test "33 suite: refund case clears the policy scorer" {
    assert.contains(s33_SupportReply_manual("refund?"), "refund")
  }
  test "33 suite: hours case clears the exact-match scorer" {
    let out = s33_SupportReply_manual("hours?", provider = BepV2FixtureGenerate { name: "case", reply: `"9am-5pm"`, attributes: {} });
    assert.equal(out, "9am-5pm")
  }
  test "33 suite: judge approves a good answer" {
    assert.is_true(s33_Judge_manual("q", "good").pass)
  }
  test "33 suite: a policy regression is tolerated under the threshold" {
    let out = s33_SupportReply_manual("returns?", provider = BepV2FixtureGenerate { name: "case", reply: `"whenever"`, attributes: {} });
    assert.contains(out, "policy")
  }
}

testset "integ-test-33-evaluation" {
  test "33 live: a real judge returns a typed Verdict on a real task's output" {
    let answer = s33_SupportReply_manual("How long do I have to return an item?", provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    });
    let verdict = s33_Judge_manual("How long do I have to return an item?", answer, provider = root.ai.Anthropic {
      model: "claude-haiku-4-5-20251001", api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      base_url: null, max_tokens: null, anthropic_version: null,
      extra_headers: null, extra_body: null, append_output_schema: null,
    });
    assert.is_true(verdict.score >= 1 && verdict.score <= 5)
  }
}
