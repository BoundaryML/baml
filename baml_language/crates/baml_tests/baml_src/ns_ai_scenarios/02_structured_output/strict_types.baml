// Additional OpenAI strict return-type coverage. Each helper below is a
// hand-written reference for what the corresponding deferred LLM function
// would desugar to; `_manual` is test scaffolding, not a generated companion.

enum S02Risk {
  Low
  Medium
  High
}

enum S02WorkState {
  Planned
  Active
  Done
}

class S02WorkItem {
  title: string,
  risk: S02Risk,
  state: S02WorkState,
}

class S02TeamNode {
  name: string,
  risk: S02Risk,
  reports: S02TeamNode[],
}

// Manual expansion reference for a deferred `s02_ReturnBool() -> bool` LLM
// function. OpenAiStrict wraps the primitive under the synthetic tool's
// required `value` property.
function s02_ReturnBool_request(provider: root.ai.Provider) -> root.ai.Request<bool> {
  root.ai.task_request<bool>(
    provider,
    "s02_ReturnBool",
    "ns_ai_scenarios",
    {},
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template("Return the boolean value true.", true),
  )
}

function s02_ReturnBool_manual(provider: root.ai.Provider) -> bool
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<bool>(s02_ReturnBool_request(provider))
}

// Manual expansion reference for a deferred `s02_ReturnString() -> string`
// LLM function.
function s02_ReturnString_request(provider: root.ai.Provider) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s02_ReturnString",
    "ns_ai_scenarios",
    {},
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Return exactly the string "lunar-ready" with no other text.`, true),
  )
}

function s02_ReturnString_manual(provider: root.ai.Provider) -> string
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s02_ReturnString_request(provider))
}

// Manual expansion reference for a deferred `s02_ClassifyRisk() -> S02Risk`
// LLM function. This exercises an enum as the root return value.
function s02_ClassifyRisk_request(provider: root.ai.Provider) -> root.ai.Request<S02Risk> {
  root.ai.task_request<S02Risk>(
    provider,
    "s02_ClassifyRisk",
    "ns_ai_scenarios",
    {},
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template("Classify a production database outage as Low, Medium, or High risk. Return High.", true),
  )
}

function s02_ClassifyRisk_manual(provider: root.ai.Provider) -> S02Risk
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<S02Risk>(s02_ClassifyRisk_request(provider))
}

// Manual expansion reference for a deferred
// `s02_ExtractWorkItem() -> S02WorkItem` LLM function. The class contains two
// independently constrained enum fields.
function s02_ExtractWorkItem_request(provider: root.ai.Provider) -> root.ai.Request<S02WorkItem> {
  root.ai.task_request<S02WorkItem>(
    provider,
    "s02_ExtractWorkItem",
    "ns_ai_scenarios",
    {},
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(
      `Create a work item titled "Ship lunar release". Its risk is High and its state is Active.`,
      true,
    ),
  )
}

function s02_ExtractWorkItem_manual(provider: root.ai.Provider) -> S02WorkItem
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<S02WorkItem>(s02_ExtractWorkItem_request(provider))
}

// Manual expansion reference for a deferred
// `s02_ListDecisions() -> S02Decision[]` LLM function. This remains one strict
// tool call whose `value` is an ordered array; it is not parallel tool use.
function s02_ListDecisions_request(provider: root.ai.Provider) -> root.ai.Request<S02Decision[]> {
  root.ai.task_request<S02Decision[]>(
    provider,
    "s02_ListDecisions",
    "ns_ai_scenarios",
    {},
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(
      `Return exactly two decisions in order. First: approved, summary "Launch approved". Second: rejected, reason "Missing review".`,
      true,
    ),
  )
}

function s02_ListDecisions_manual(provider: root.ai.Provider) -> S02Decision[]
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<S02Decision[]>(s02_ListDecisions_request(provider))
}

// Manual expansion reference for a deferred
// `s02_ExtractTeam() -> S02TeamNode` LLM function. This combines a recursive
// class graph with an enum at every node.
function s02_ExtractTeam_request(provider: root.ai.Provider) -> root.ai.Request<S02TeamNode> {
  root.ai.task_request<S02TeamNode>(
    provider,
    "s02_ExtractTeam",
    "ns_ai_scenarios",
    {},
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(
      `Build this reporting tree: Ada has High risk and one report, Grace. Grace has Medium risk and no reports.`,
      true,
    ),
  )
}

function s02_ExtractTeam_manual(provider: root.ai.Provider) -> S02TeamNode
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<S02TeamNode>(s02_ExtractTeam_request(provider))
}

testset "integ-test-02-openai-strict-types" {
  test "02 live: OpenAI strict returns a root bool" {
    let value = s02_ReturnBool_manual(root.ai.OpenAiStrict {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    });
    assert.equal(value, true)
  }

  test "02 live: OpenAI strict returns a root string" {
    let value = s02_ReturnString_manual(root.ai.OpenAiStrict {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    });
    assert.equal(value, "lunar-ready")
  }

  test "02 live: OpenAI strict returns a root enum" {
    let value = s02_ClassifyRisk_manual(root.ai.OpenAiStrict {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    });
    assert.equal(value, S02Risk.High)
  }

  test "02 live: OpenAI strict returns a class containing enums" {
    let item = s02_ExtractWorkItem_manual(root.ai.OpenAiStrict {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    });
    assert.equal(item.title, "Ship lunar release");
    assert.equal(item.risk, S02Risk.High);
    assert.equal(item.state, S02WorkState.Active)
  }

  test "02 live: OpenAI strict returns one mixed list of union values" {
    let decisions = s02_ListDecisions_manual(root.ai.OpenAiStrict {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    });
    assert.equal(decisions.length(), 2);
    assert.is_true(decisions[0] is S02Approved);
    assert.is_true(decisions[1] is S02Rejected)
  }

  test "02 live: OpenAI strict returns a recursive class containing enums" {
    let team = s02_ExtractTeam_manual(root.ai.OpenAiStrict {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    });
    assert.equal(team.name, "Ada");
    assert.equal(team.risk, S02Risk.High);
    assert.equal(team.reports.length(), 1);
    assert.equal(team.reports[0].name, "Grace");
    assert.equal(team.reports[0].risk, S02Risk.Medium);
    assert.equal(team.reports[0].reports.length(), 0)
  }
}
