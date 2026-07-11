class s35_Summary { topic: string, bullets: string[] }

// Task sketch: s35_Analyze(text) -> s35_Summary.

// Hand-written reference expansion of the compiler task surface for
// `s35_Analyze`; these helper names are scaffolding, not literal generated names:
// - `s35_Analyze_request` represents `s35_Analyze.request(...)`.
// - `s35_Analyze_manual` spells the plain `s35_Analyze(...)` lowering through `root.ai.run`.
function s35_Analyze_request(
  text: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "analysis", reply: `{"topic":"fixture","bullets":[]}`, attributes: {} },
) -> root.ai.Request<s35_Summary> {
  root.ai.task_request<s35_Summary>(
    provider, "s35_Analyze", "ns_ai_scenarios", { "text": text },
    [], root.ai.RequestOptions {}, {},
    bepv2_prompt_template(`Summarize into a topic and bullets.\n${text}`, true),
  )
}

function s35_Analyze_manual(
  text: string,
  provider: root.ai.Provider = BepV2FixtureGenerate { name: "analysis", reply: `{"topic":"fixture","bullets":[]}`, attributes: {} },
) -> s35_Summary throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<s35_Summary>(s35_Analyze_request(text, provider = provider))
}

test "35: the same task body runs under different deployment homes" {
  let server = s35_Analyze_manual("launch", provider = BepV2FixtureGenerate {
    name: "server", reply: `{"topic":"Launch","bullets":["on time"]}`, attributes: {},
  });
  let local = s35_Analyze_manual("nightly", provider = BepV2FixtureGenerate {
    name: "local", reply: `{"topic":"Nightly","bullets":["passed"]}`, attributes: {},
  });
  assert.equal(server.topic, "Launch")
  assert.equal(local.topic, "Nightly")
}

test "35: deployment factories all return the same Provider root" {
  assert.is_true(s35_server_client() is root.ai.Provider)
  assert.is_true(s35_proxy_client() is root.ai.Provider)
  assert.is_true(s35_local_client() is root.ai.Provider)
}

test "35: local construction contains no credential-bearing auth" {
  let local = s35_local_client();
  let label = match (local) { let p: root.ai.OpenAiCompatible => match (p.auth) {
    let n: root.ai.NoAuth => "none", _ => "keyed",
  }, _ => "wrong" };
  assert.equal(label, "none")
}

testset "integ-test-35-deployment-shapes" {
  test "35 live: the server-with-keys home runs the app body against a real API" {
    let r = s35_Analyze_manual(
      "The billing migration completed with zero downtime.", provider = s35_server_client(),
    );
    assert.is_true(r.topic.length() > 0)
  }
}
