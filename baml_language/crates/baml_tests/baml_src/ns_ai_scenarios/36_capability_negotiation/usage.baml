// Task sketch: s36_Extract(text) -> s36_Report.

// Hand-written reference expansion of the compiler task surface for
// `s36_Extract`; these helper names are scaffolding, not literal generated names:
// - `s36_Extract_request` represents `s36_Extract.request(...)`.
// - `s36_Extract_manual` spells the plain `s36_Extract(...)` lowering.
// - `s36_Extract_stream_manual` spells `s36_Extract.stream(...)`.
function s36_Extract_request(
  text: string,
  provider: root.ai.Provider = s36_DescribedFixture { reply: `{"subject":"fixture","verdict":"ok"}`, support: root.ai.Support.Yes },
) -> root.ai.Request<s36_Report> {
  root.ai.task_request<s36_Report>(
    provider, "s36_Extract", "ns_ai_scenarios", { "text": text },
    [], root.ai.RequestOptions {}, {}, root.ai.prompt_template(`Extract a report.\n${text}`, true),
  )
}

function s36_Extract_manual(
  text: string,
  provider: root.ai.Provider = s36_DescribedFixture { reply: `{"subject":"fixture","verdict":"ok"}`, support: root.ai.Support.Yes },
) -> s36_Report throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s36_Report>(s36_Extract_request(text, provider = provider))
}

function s36_Extract_stream_manual(
  text: string,
  provider: root.ai.Provider,
) -> baml.llm.Stream<s36_Report, s36_Report> throws baml.errors.StreamError | baml.errors.UnknownError {
  root.ai.stream<s36_Report, s36_Report>(s36_Extract_request(text, provider = provider))
}

test "36: binary capability matching and graded support are separate" {
  let provider: root.ai.Provider = s36_DescribedFixture {
    reply: `{"subject":"Ada","verdict":"approved"}`, support: root.ai.Support.Conditional,
  };
  assert.is_true(provider is root.ai.Generate)
  assert.equal(s36_structured_label(provider), "conditional")
}

test "36: an ordinary driver negotiates Generate at runtime" {
  let r = s36_Extract_manual("Ada approved", provider = s36_DescribedFixture {
    reply: `{"subject":"Ada","verdict":"approved"}`, support: root.ai.Support.Yes,
  });
  assert.equal(r.verdict, "approved")
}

test "36: streaming absence is a typed capability error" {
  let label = s36_Extract_stream_manual("x", s36_DescribedFixture {
    reply: `{"subject":"x","verdict":"ok"}`, support: root.ai.Support.Yes,
  }) catch (e) {
    let u: baml.errors.Unsupported => "unsupported",
    _ => "wrong error",
  };
  assert.equal(label, "unsupported")
}

testset "integ-test-36-capability-negotiation" {
  test "36 live: OpenAiStrict advertises its implemented strict capabilities" {
    let open: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    let strict: root.ai.Provider = root.ai.OpenAiStrict {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    assert.is_true(open is root.ai.Streaming)
    assert.is_true(open is root.ai.Capabilities)
    assert.is_true(strict is root.ai.Capabilities)
    assert.is_true(strict is root.ai.Tools)
    assert.is_true(!(strict is root.ai.Streaming))
  }

  test "36 live: one real call flows through the negotiated Yes arm" {
    let r = s36_Extract_manual("Ada Lovelace report: approved.", provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    });
    assert.is_true(r.subject.length() > 0)
  }
}
