test "30: a passing cheap answer avoids escalation" {
  let r = s30_gated_cascade(
    root.ai.FakeGenerateProvider { name: "cheap", reply: `"Paris is the capital."`, attributes: {} },
    root.ai.FakeGenerateProvider { name: "strong", reply: `"unused"`, attributes: {} },
    "Capital of France?", "Paris",
  );
  assert.equal(r.tier, "cheap")
  assert.is_true(!r.escalated)
}

test "30: a failed quality gate escalates to the strong provider" {
  let r = s30_gated_cascade(
    root.ai.FakeGenerateProvider { name: "cheap", reply: `"unsure"`, attributes: {} },
    root.ai.FakeGenerateProvider { name: "strong", reply: `"Paris"`, attributes: {} },
    "Capital of France?", "Paris",
  );
  assert.equal(r.answer, "Paris")
  assert.equal(r.tier, "strong")
}

test "30: routing selects a provider before request rendering" {
  let chosen = s30_route(false,
    root.ai.FakeGenerateProvider { name: "cheap", reply: `"cheap"`, attributes: {} },
    root.ai.FakeGenerateProvider { name: "strong", reply: `"strong"`, attributes: {} },
  );
  let request = s30_Answer_request("hello", provider = chosen);
  assert.equal(request.provider_name(), "cheap")
}

testset "integ-test-30-cascades-routing" {
  test "30 live: the cheap tier settles when its answer passes the gate" {
    let model: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    let r = s30_gated_cascade(model, model, "Reply with exactly the word Paris.", "Paris");
    assert.equal(r.tier, "cheap")
  }

  test "30 live: a failed gate escalates to the strong tier" {
    let cheap: root.ai.Provider = root.ai.FakeGenerateProvider { name: "cheap", reply: `"wrong"`, attributes: {} };
    let strong: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    };
    let r = s30_gated_cascade(cheap, strong, "Reply with exactly the word Paris.", "Paris");
    assert.equal(r.tier, "strong")
    assert.is_true(r.answer.to_lower_case().includes("paris"))
  }
}
