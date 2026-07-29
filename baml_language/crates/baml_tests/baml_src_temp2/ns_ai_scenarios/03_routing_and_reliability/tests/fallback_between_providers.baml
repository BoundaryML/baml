// Tests for the fallback_between_providers scenario.
test "guide 03.02: fallback re-renders and tries the next provider" {
  //# Configure a failing primary and healthy fallback
  let failed = root.ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 2 };
  let good = root.ai.testing.FakeProvider { ...fake_resolution(), provider_name: "second" };
  //# Execute the ordered fallback chain
  let task = ResolveTicket_task(sample_ticket(), root.ai.fallback([failed, good]));
  let response = root.ai.internal._response_from_done(
    root.ai.internal._require_done(
      task.run(runner = root.ai.run.Agent<Resolution>.new()),
    ),
  );
  //# Verify the fallback provider completed
  assert.equal(response.metadata.provider, "second")
}

test "guide 03.02: fallback requires at least one provider" {
  let rejected = false;
  root.ai.fallback([]) catch (e) {
    let invalid: baml.errors.InvalidArgument => {
      rejected = invalid.message.includes("at least one provider");
      null
    },
  };
  assert.is_true(rejected)
}

function _fallback_wrong_owner_step(
  provider: root.ai.Fallback,
  conversation: root.ai.Conversation,
) -> string throws never {
  provider.step<Resolution>(conversation, []) catch (e) {
    let unknown: baml.errors.UnknownError => return "rejected",
    _ => return "other",
  };
  "accepted"
}

test "guide 03.02: fallback conversations require the exact wrapper instance" {
  let inner = fake_resolution();
  let owner = root.ai.fallback([inner]);
  let other = root.ai.fallback([inner]);
  let conversation = owner.begin<Resolution>(
    ResolveTicket_task(sample_ticket(), owner),
  );
  assert.equal(_fallback_wrong_owner_step(other, conversation), "rejected")
}

testset "integ-test-guide-03-fallback" {
  test "guide 03.02 live Anthropic: fallback reaches a real second provider" {
    //# Configure a failing fake and live fallback
    let providers: root.ai.Provider[] = [
      {
        root.ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 }
      },
      root.anthropic.AnthropicProvider {
        ...root.anthropic.messages(),
        model: "claude-sonnet-4-6",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      },
    ];
    //# Execute the live fallback chain
    let task = ResolveTicket_task(sample_ticket(), root.ai.fallback(providers));
    let response = root.ai.internal._response_from_done(
      root.ai.internal._require_done(
        task.run(runner = root.ai.run.Agent<Resolution>.new()),
      ),
    );
    //# Verify Anthropic completed the task
    assert.equal(response.metadata.provider, "anthropic")
  }
}
