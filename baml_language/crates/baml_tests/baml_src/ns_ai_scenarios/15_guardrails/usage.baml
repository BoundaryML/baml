// 15 — guardrails are wrappers around Generate, not task modifiers.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s15_FileTaxes(question: string) -> string {
//   client: s15_Guarded {
//     inner: BepV2FixtureGenerate { name: "tax-model", reply: `"four"`, attributes: {} },
//     blocked_phrase: "SSN",
//   }
//   prompt: `Answer this tax question: ${question}`
// }

// Manual expansion reference for task `s15_FileTaxes`:
// - `s15_FileTaxes_request(...)` models `s15_FileTaxes.request(...)`.
// - `s15_FileTaxes_manual(...)` models the plain `s15_FileTaxes(...)` call,
//   lowered through `root.ai.run`.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s15_FileTaxes_request(
  question: string,
  provider: root.ai.Provider = s15_Guarded {
    inner: BepV2FixtureGenerate { name: "tax-model", reply: `"four"`, attributes: {} },
    blocked_phrase: "SSN",
  },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s15_FileTaxes",
    "ns_ai_scenarios",
    { "question": question },
    [],
    root.ai.RequestOptions {},
    {},
    bepv2_prompt_template(`Answer this tax question: ${question}`, false),
  )
}

function s15_FileTaxes_manual(
  question: string,
  provider: root.ai.Provider = s15_Guarded {
    inner: BepV2FixtureGenerate { name: "tax-model", reply: `"four"`, attributes: {} },
    blocked_phrase: "SSN",
  },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s15_FileTaxes_request(question, provider = provider))
}

test "15: a clean request passes through the wrapper" {
  assert.equal(s15_FileTaxes_manual("what is two plus two?"), "four")
}

test "15: a tripwire is a typed policy refusal before generation" {
  let status = s15_FileTaxes_manual("My SSN is 123") catch (e) {
    let ignored: s15_InputTripwire => "refused",
    _ => "wrong-error",
  };
  assert.equal(status, "refused")
}

testset "integ-test-15-guardrails" {
  test "15 live: a guarded real provider accepts a clean prompt" {
    let answer = s15_FileTaxes_manual(
      "In one word, what is 2+2?",
      provider = s15_Guarded {
        inner: root.ai.OpenAi {
          model: "gpt-5.6-luna",
          api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
          base_url: null,
        },
        blocked_phrase: "SSN",
      },
    );
    assert.is_true(answer.length() > 0)
  }
}
