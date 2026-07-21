test "guide 08.01: a harness owns its session while returning typed output" {
  //# Configure an in-process model harness
  let harness = fake_model_harness();
  //# Submit a typed task through a harness session
  let run = InvestigateRepository_task(sample_ticket(), harness.inner).drive(
    root.ai.driver.harness<Resolution>(harness, harness_options()),
  );
  //# Verify output and resumable token ownership
  assert.equal(run.value.category, "billing")
  assert.equal(run.token.runtime, "baml")
}

test "guide 08.01: the harness task carries the evidence needed by the model" {
  //# Render the harness investigation task
  let task = InvestigateRepository_task(sample_ticket(), fake_resolution());
  let prompt = baml.llm.prompt_to_text(task.prompt);
  //# Verify the task carries repository evidence
  assert.contains(prompt, "Ticket: T-100")
  assert.contains(prompt, "Subject: Charged twice")
  assert.contains(prompt, "Body: I see two charges for order O-42.")
  assert.contains(prompt, "Customer tier: pro")
}

testset "integ-test-guide-08-model-harness" {
  test "guide 08.01 live OpenAI: real model behind the harness contract" {
    //# Configure a live OpenAI-backed harness
    let harness = root.ai.ModelHarness {
      inner: root.ai.OpenAi {
        ...root.ai.openai(),
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      },
      sessions: [],
    };
    //# Execute a typed harness run
    let run = InvestigateRepository_task(sample_ticket(), harness.inner).drive(
      root.ai.driver.harness<Resolution>(harness, harness_options()),
    );
    //# Verify live typed output
    assert.is_true(run.value.reply.length() > 0)
  }

  test "guide 08.01 live Anthropic: same harness adapter" {
    //# Configure a live Anthropic-backed harness
    let harness = root.ai.ModelHarness {
      inner: root.ai.Anthropic {
        ...root.ai.anthropic(),
        model: "claude-sonnet-4-6",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      },
      sessions: [],
    };
    //# Execute through the same harness contract
    let run = InvestigateRepository_task(sample_ticket(), harness.inner).drive(
      root.ai.driver.harness<Resolution>(harness, harness_options()),
    );
    //# Verify live typed output
    assert.is_true(run.value.summary.length() > 0)
  }
}
