// Tests for the fallback_between_providers scenario.
test "guide 03.02: fallback re-renders and tries the next provider" {
  //# Configure a failing primary and healthy fallback
  let failed = ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 2 };
  let good = ai.testing.FakeProvider { ...fake_resolution(), provider_name: "second" };
  //# Execute the ordered fallback chain
  let task = ResolveTicket@task(sample_ticket()).with_provider(ai.fallback([failed, good]));
  let response = (
    done_or_fail(
      task.run(runner = ai.run.Agent<Resolution>.new()),
    )
  ).response();
  //# Verify the fallback provider completed
  assert.equal(response.metadata.provider, "second")
}

test "guide 03.02: fallback requires at least one provider" {
  let rejected = false;
  ai.fallback([]) catch (e) {
    let invalid: baml.errors.InvalidArgument => {
      rejected = invalid.message.includes("at least one provider");
      null
    },
  };
  assert.is_true(rejected)
}

function _fallback_wrong_owner_step(
  provider: ai.Fallback,
  conversation: ai.Conversation,
) -> string throws never {
  provider.step<Resolution>(conversation, []) catch (e) {
    let unknown: baml.errors.UnknownError => return "rejected",
    _ => return "other",
  };
  "accepted"
}

test "guide 03.02: fallback conversations require the exact wrapper instance" {
  let inner = fake_resolution();
  let owner = ai.fallback([inner]);
  let other = ai.fallback([inner]);
  let conversation = owner.begin<Resolution>(
    ResolveTicket@task(sample_ticket()).with_provider(owner),
  );
  assert.equal(_fallback_wrong_owner_step(other, conversation), "rejected")
}

testset "integ-test-guide-03-fallback" {
  test "guide 03.02 live Anthropic: fallback reaches a real second provider" {
    //# Configure a failing fake and live fallback
    let providers: ai.Provider[] = [
      {
        ai.testing.FakeProvider { ...fake_resolution(), failures_remaining: 1 }
      },
      root.anthropic.AnthropicProvider {
        ...root.anthropic.messages(),
        model: "claude-sonnet-4-6",
        api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      },
    ];
    //# Execute the live fallback chain
    let task = ResolveTicket@task(sample_ticket()).with_provider(ai.fallback(providers));
    let response = (
      done_or_fail(
        task.run(runner = ai.run.Agent<Resolution>.new()),
      )
    ).response();
    //# Verify Anthropic completed the task
    assert.equal(response.metadata.provider, "anthropic")
  }
}
