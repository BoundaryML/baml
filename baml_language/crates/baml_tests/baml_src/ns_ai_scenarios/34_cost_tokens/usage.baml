// Task sketch: s34_Classify(text) -> s34_Ticket.

// Hand-written reference expansion of the compiler task surface for
// `s34_Classify`; these helper names are scaffolding, not literal generated names:
// - `s34_Classify_request` represents `s34_Classify.request(...)`.
// - `s34_Classify_manual` spells the plain `s34_Classify(...)` lowering.
// - `s34_Classify_with_meta_manual` spells `s34_Classify.with_meta(...)`.
function s34_Classify_request(
  text: string,
  provider: root.ai.Provider = root.ai.metered_fake(
    `{"category":"other","severity":1,"summary":"fake"}`, 1, 1, name = "metered",
  ),
) -> root.ai.Request<s34_Ticket> {
  root.ai.task_request<s34_Ticket>(
    provider, "s34_Classify", "ns_ai_scenarios", { "text": text },
    [], root.ai.RequestOptions {}, {}, root.ai.prompt_template(`Classify the ticket.\n${text}`, true),
  )
}

function s34_Classify_manual(
  text: string,
  provider: root.ai.Provider = root.ai.metered_fake(
    `{"category":"other","severity":1,"summary":"fake"}`, 1, 1, name = "metered",
  ),
) -> s34_Ticket throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s34_Ticket>(s34_Classify_request(text, provider = provider))
}

function s34_Classify_with_meta_manual(
  text: string,
  provider: root.ai.Provider = root.ai.metered_fake(
    `{"category":"other","severity":1,"summary":"fake"}`, 1, 1, name = "metered",
  ),
) -> root.ai.Response<s34_Ticket> throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run_with_meta<s34_Ticket>(s34_Classify_request(text, provider = provider))
}

test "34: normalized usage can be priced by application policy" {
  let r = s34_Classify_with_meta_manual(
    "late fee",
    provider = root.ai.metered_fake(
      `{"category":"billing","severity":3,"summary":"late fee"}`,
      1200,
      400,
      name = "metered",
    ),
  );
  let cost = (r.meta.usage ?? root.ai.Usage { input_tokens: 0, output_tokens: 0 }).estimated_cost(root.ai.TokenPrice {
    input_per_million: 0.4, output_per_million: 1.6,
  });
  assert.equal(r.value.category, "billing")
  assert.is_true(cost > 0.00111 && cost < 0.00113)
}

test "34: a batch consumes keyed semantic requests" {
  let provider: root.ai.Provider = s34_BatchProvider {};
  let requests = [
    s34_Classify_request("a", provider = provider),
    s34_Classify_request("b", provider = provider),
  ];
  let batch = root.ai.submit_batch<s34_Ticket>(provider, requests, (r, i) -> { `ticket-${i}` });
  assert.equal(batch.id, "batch-2")
  assert.equal(batch.owner, "batch-fake")
}

testset "integ-test-34-cost-tokens" {
  test "34 live: a real $with call projects usage and prices it via the rate table" {
    let r = s34_Classify_with_meta_manual("Refund requested: the package arrived damaged.", provider = root.ai.OpenAi {
      model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
    });
    let usage = r.meta.usage ?? root.ai.Usage { input_tokens: 0, output_tokens: 0 };
    let cost = usage.estimated_cost(root.ai.TokenPrice {
      input_per_million: 0.4,
      output_per_million: 1.6,
    });
    assert.is_true(usage.input_tokens > 0)
    assert.is_true(cost > 0.0)
  }
}
