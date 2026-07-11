// 02 — structured output stays on the baseline Generate capability.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s02_ExtractEvent(text: string) -> S02Event {
//   client: BepV2FixtureGenerate {
//     name: "structured-default",
//     reply: `{"title":"Launch","date":"2026-01-01","attendees":["Ada"]}`,
//     attributes: {},
//   }
//   prompt: `Extract the event from ${text}. ${ctx.output_format}`
// }

// Manual expansion reference for s02_ExtractEvent.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s02_ExtractEvent_request(...) models s02_ExtractEvent.request(...): it
//   packages Request<S02Event>, including its structured output schema, without I/O.
// - s02_ExtractEvent_manual(...) models the plain s02_ExtractEvent(...) lowering:
//   root.ai.run(request), returning the parsed S02Event.
function s02_ExtractEvent_request(
  text: string,
  provider: root.ai.Provider = BepV2FixtureGenerate {
    name: "structured-default",
    reply: `{"title":"Launch","date":"2026-01-01","attendees":["Ada"]}`,
    attributes: {},
  },
) -> root.ai.Request<S02Event> {
  root.ai.task_request<S02Event>(
    provider,
    "s02_ExtractEvent",
    "ns_ai_scenarios",
    { "text": text },
    [],
    root.ai.RequestOptions {},
    {},
    bepv2_prompt_template(`Extract the event from ${text}.`, true),
  )
}

function s02_ExtractEvent_manual(
  text: string,
  provider: root.ai.Provider = BepV2FixtureGenerate {
    name: "structured-default",
    reply: `{"title":"Launch","date":"2026-01-01","attendees":["Ada"]}`,
    attributes: {},
  },
) -> S02Event throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<S02Event>(s02_ExtractEvent_request(text, provider = provider))
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s02_ExtractOrg(text: string) -> S02OrgNode {
//   client: BepV2FixtureGenerate {
//     name: "recursive-default",
//     reply: `{"name":"CEO","reports":[]}`,
//     attributes: {},
//   }
//   prompt: `Extract the reporting tree from ${text}. ${ctx.output_format}`
// }

// Manual expansion reference for s02_ExtractOrg.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s02_ExtractOrg_request(...) models s02_ExtractOrg.request(...): it packages
//   Request<S02OrgNode>, including its recursive output schema, without I/O.
// - s02_ExtractOrg_manual(...) models the plain s02_ExtractOrg(...) lowering:
//   root.ai.run(request), returning the parsed S02OrgNode.
function s02_ExtractOrg_request(
  text: string,
  provider: root.ai.Provider = BepV2FixtureGenerate {
    name: "recursive-default",
    reply: `{"name":"CEO","reports":[]}`,
    attributes: {},
  },
) -> root.ai.Request<S02OrgNode> {
  root.ai.task_request<S02OrgNode>(
    provider,
    "s02_ExtractOrg",
    "ns_ai_scenarios",
    { "text": text },
    [],
    root.ai.RequestOptions {},
    {},
    bepv2_prompt_template(`Extract the reporting tree from ${text}.`, true),
  )
}

function s02_ExtractOrg_manual(
  text: string,
  provider: root.ai.Provider = BepV2FixtureGenerate {
    name: "recursive-default",
    reply: `{"name":"CEO","reports":[]}`,
    attributes: {},
  },
) -> S02OrgNode throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<S02OrgNode>(s02_ExtractOrg_request(text, provider = provider))
}

test "02: the manual request preserves the typed return schema" {
  let event = s02_ExtractEvent_manual("Ada attends Launch on 2026-01-01");
  assert.equal(event.title, "Launch")
  assert.equal(event.attendees, ["Ada"])
}

test "02: recursive schemas are ordinary typed results" {
  assert.equal(s02_ExtractOrg_manual("The CEO has no reports").name, "CEO")
}

testset "integ-test-02-structured" {
  test "02 live: structured extraction via OpenAI strict json_schema" {
    let source = "Ada and Bob meet for Launch on 2026-01-01.";
    let openai = s02_ExtractEvent_manual(source, provider = root.ai.OpenAiStrict {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    });
    assert.equal(openai.date, "2026-01-01")
  }

  test "02 live: the same type through OpenAI Responses text.format" {
    let source = "Ada and Bob meet for Launch on 2026-01-01.";
    let responses = s02_ExtractEvent_manual(source, provider = root.ai.OpenAiResponses {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
      append_output_schema: null,
      store: null,
    });
    assert.equal(responses.date, "2026-01-01")
  }

  test "02 live: the same type through Anthropic schema append" {
    let source = "Ada and Bob meet for Launch on 2026-01-01.";
    let anthropic = s02_ExtractEvent_manual(source, provider = root.ai.Anthropic {
      model: "claude-haiku-4-5-20251001",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      base_url: null,
      max_tokens: null,
      anthropic_version: null,
      extra_headers: null,
      extra_body: null,
      append_output_schema: null,
    });
    assert.equal(anthropic.date, "2026-01-01")
  }

  test "02 live: the same type through Gemini responseSchema" {
    let source = "Ada and Bob meet for Launch on 2026-01-01.";
    let gemini = s02_ExtractEvent_manual(source, provider = root.ai.Gemini {
      model: "gemini-flash-latest",
      api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
      append_output_schema: null,
    }.with_retry(2));
    assert.equal(gemini.date, "2026-01-01")
  }
}
