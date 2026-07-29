
test "guide 08.01: a harness owns its session while returning typed output" {
  //# Configure an in-process model harness
  let harness = fake_model_harness();
  //# Submit a typed task through a harness session
  let run = InvestigateRepository_task(sample_ticket(), harness.inner).run(
    runner = root.ai.run.Harness<Resolution>.new(
      harness,
      cwd = "/workspace",
      permission_mode = "read-only",
      sandbox = "workspace",
      attributes = { "team": "support" },
    ),
  );
  //# Verify output and resumable token ownership
  assert.equal(run.value.category, "billing")
  assert.equal(run.token.runtime, "baml")
}

test "guide 08.01: the harness task carries the evidence needed by the model" {
  //# Render the harness investigation task
  let task = InvestigateRepository_task(sample_ticket(), fake_resolution());
  let prompt = task.prompt.text();
  //# Verify the task carries repository evidence
  assert.contains(prompt, "Ticket: T-100")
  assert.contains(prompt, "Subject: Charged twice")
  assert.contains(prompt, "Body: I see two charges for order O-42.")
  assert.contains(prompt, "Customer tier: pro")
}

testset "integ-test-guide-08-model-harness" {
  test "guide 08.01 live OpenAI: real model behind the harness contract" {
    //# Configure a live OpenAI-backed harness
    let harness = root.ai.harness.ModelHarness {
      inner: root.openai.OpenAIProvider {
        ...root.openai.responses(),
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      },
      sessions: [],
    };
    //# Execute a typed harness run
    let run = InvestigateRepository_task(sample_ticket(), harness.inner).run(
      runner = root.ai.run.Harness<Resolution>.new(
        harness,
        cwd = "/workspace",
        permission_mode = "read-only",
        sandbox = "workspace",
        attributes = { "team": "support" },
      ),
    );
    //# Verify live typed output
    assert.is_true(run.value.reply.length() > 0)
  }

  test "guide 08.01 live Anthropic: same harness adapter" {
    //# Configure a live Anthropic-backed harness
    let harness = root.ai.harness.ModelHarness {
      inner: root.anthropic.AnthropicProvider {
        ...root.anthropic.messages(),
        model: "claude-sonnet-4-6",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      },
      sessions: [],
    };
    //# Execute through the same harness contract
    let run = InvestigateRepository_task(sample_ticket(), harness.inner).run(
      runner = root.ai.run.Harness<Resolution>.new(
        harness,
        cwd = "/workspace",
        permission_mode = "read-only",
        sandbox = "workspace",
        attributes = { "team": "support" },
      ),
    );
    //# Verify live typed output
    assert.is_true(run.value.summary.length() > 0)
  }
}
