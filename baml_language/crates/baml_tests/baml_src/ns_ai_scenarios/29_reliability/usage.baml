// Task sketch: s29_Extract(work) -> string.

// Hand-written reference expansion of the compiler task surface for
// `s29_Extract`; these helper names are scaffolding, not literal generated names:
// - `s29_Extract_request` represents `s29_Extract.request(...)`.
// - `s29_Extract_manual` spells the plain `s29_Extract(...)` lowering through `root.ai.run`.
function s29_Extract_request(
  work: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "reliable", reply: `"done"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s29_Extract", "ns_ai_scenarios", { "work": work },
    [], root.ai.RequestOptions {}, {}, bepv2_prompt_template(work, false),
  )
}

function s29_Extract_manual(
  work: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "reliable", reply: `"done"`, attributes: {} },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s29_Extract_request(work, provider = provider))
}

test "29: retry replays a safe transient failure" {
  let flaky = s29_Flaky { reply: `"recovered"`, fail_until: 1, calls: 0 };
  let answer = s29_Extract_manual("work", provider = flaky.with_retry(2));
  assert.equal(answer, "recovered")
  assert.equal(flaky.calls, 2)
}

test "29: may_replay combines the operation policy with failure facts" {
  let failure: root.ai.Failure = s29_Transient {};
  assert.is_true(root.ai.may_replay(root.ai.ReplayPolicy {
    kind: root.ai.ReplayKind.Safe, idempotency_key: null,
  }, failure))
  assert.is_true(!root.ai.may_replay(root.ai.ReplayPolicy {
    kind: root.ai.ReplayKind.Never, idempotency_key: null,
  }, failure))
}

test "29: fallback selects a later provider after a replay-safe failure" {
  let chain = s29_Flaky { reply: `"never"`, fail_until: 99, calls: 0 }.fallback_to(
    BepV2FixtureGenerate { name: "backup", reply: `"backup"`, attributes: {} },
  );
  assert.equal(s29_Extract_manual("work", provider = chain), "backup")
}

testset "integ-test-29-reliability" {
  test "29 live: Fallback fails over a dead primary to real OpenAI" {
    let chain = s29_Flaky { reply: `"never"`, fail_until: 99, calls: 0 }.fallback_to(root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    });
    let r = s29_Extract_manual("Reply with exactly the lowercase word pong.", provider = chain);
    assert.is_true(r.to_lower_case().includes("pong"))
  }

  test "29 live: a retry-wrapped real OpenAI still returns on the first healthy attempt" {
    let model: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    }.with_retry(2);
    let r = s29_Extract_manual("Reply with exactly the lowercase word pong.", provider = model);
    assert.is_true(r.to_lower_case().includes("pong"))
  }
}
