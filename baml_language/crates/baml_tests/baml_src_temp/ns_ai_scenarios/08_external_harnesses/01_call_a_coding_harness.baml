test "guide 08.01: a harness owns its session while returning typed output" {
  let harness = fake_model_harness();
  let run = root.ai.drivers.run_harness<Resolution>(
    InvestigateRepository_task(sample_ticket(), harness),
    harness_options(),
  );
  assert.equal(run.value.category, "billing")
  assert.equal(run.token.runtime, "baml")
}

testset "integ-test-guide-08-model-harness" {
  test "guide 08.01 live OpenAI: real model behind the harness contract" {
    let harness = root.ai.ModelHarness {
      inner: root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null, extra_headers: null, extra_body: null,
      },
      sessions: [],
    };
    let run = root.ai.drivers.run_harness<Resolution>(
      InvestigateRepository_task(sample_ticket(), harness),
      harness_options(),
    );
    assert.is_true(run.value.reply.length() > 0)
  }

  test "guide 08.01 live Anthropic: same harness adapter" {
    let harness = root.ai.ModelHarness {
      inner: root.ai.Anthropic {
        model: "claude-sonnet-4-6",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
        base_url: null, extra_headers: null, extra_body: null,
      },
      sessions: [],
    };
    let run = root.ai.drivers.run_harness<Resolution>(
      InvestigateRepository_task(sample_ticket(), harness),
      harness_options(),
    );
    assert.is_true(run.value.summary.length() > 0)
  }
}

