// Task sketch: s32_Summarize(text) -> string.

// Hand-written reference expansion of the compiler task surface for
// `s32_Summarize`; these helper names are scaffolding, not literal generated names:
// - `s32_Summarize_request` represents `s32_Summarize.request(...)`.
// - `s32_Summarize_manual` spells the plain `s32_Summarize(...)` lowering.
// - `s32_Summarize_with_meta_manual` spells `s32_Summarize.with_meta(...)`.
function s32_Summarize_request(
  text: string,
  provider: root.ai.Provider = root.ai.metered_fixture(`"summary"`, 1, 1, name = "default"),
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s32_Summarize", "ns_ai_scenarios", { "text": text },
    [], root.ai.RequestOptions {}, {}, root.ai.prompt_template(`Summarize: ${text}`, false),
  )
}

function s32_Summarize_manual(
  text: string,
  provider: root.ai.Provider = root.ai.metered_fixture(`"summary"`, 1, 1, name = "default"),
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s32_Summarize_request(text, provider = provider))
}

function s32_Summarize_with_meta_manual(
  text: string,
  provider: root.ai.Provider = root.ai.metered_fixture(`"summary"`, 1, 1, name = "default"),
) -> root.ai.Response<string> throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run_with_meta<string>(s32_Summarize_request(text, provider = provider))
}

test "32: with-meta returns value and normalized usage from one call" {
  let r = s32_Summarize_with_meta_manual(
    "quarter",
    provider = root.ai.metered_fixture(`"revenue up"`, 120, 40, name = "openai-shaped"),
  );
  assert.equal(r.value, "revenue up")
  assert.equal(r.meta.usage?.input_tokens, 120)
  assert.equal(r.meta.finish_reason, "stop")
}

test "32: metadata projection happens after provider execution" {
  let r = s32_Summarize_with_meta_manual("hello");
  let label = `${r.meta.provider}/${r.meta.usage?.output_tokens ?? 0}`;
  assert.equal(label, "default/1")
}

testset "integ-test-32-observability" {
  test "32 live: $with over real OpenAI projects real usage (input+output tokens > 0)" {
    let r = s32_Summarize_with_meta_manual("Reply with exactly the lowercase word pong.", provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    });
    assert.is_true((r.meta.usage?.input_tokens ?? 0) > 0)
    assert.is_true((r.meta.usage?.output_tokens ?? 0) > 0)
  }

  test "32 live: a .traced(meter) real call records real usage into the meter" {
    let meter = root.ai.UsageMeter { entries: [] };
    let model: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    }.traced(meter);
    let _ = s32_Summarize_manual("Reply with exactly pong.", provider = model);
    assert.equal(meter.calls(), 1)
    assert.is_true(meter.total().input_tokens > 0)
  }
}
