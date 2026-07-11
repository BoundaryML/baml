// 18 — application-owned compaction is an ordinary task plus array operations.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s18_SummarizeHead(head: string) -> string {
//   client: root.ai.FixtureGenerate {
//     name: "summary-default",
//     reply: `"Venue and catering are booked."`,
//     attributes: {},
//   }
//   prompt: `Condense this conversation, preserving decisions and open work: ${head}`
// }

// Manual expansion reference for task `s18_SummarizeHead`:
// - `s18_SummarizeHead_request(...)` models `s18_SummarizeHead.request(...)`.
// - `s18_SummarizeHead_manual(...)` models the plain
//   `s18_SummarizeHead(...)` call, lowered through `root.ai.run`.
// The surrounding compaction logic is ordinary application code. These helper
// names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s18_SummarizeHead_request(
  head: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
    name: "summary-default",
    reply: `"Venue and catering are booked."`,
    attributes: {},
  },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s18_SummarizeHead",
    "ns_ai_scenarios",
    { "head": head },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(
      `Condense this conversation, preserving decisions and open work: ${head}`,
      false,
    ),
  )
}

function s18_SummarizeHead_manual(
  head: string,
  provider: root.ai.Provider = root.ai.FixtureGenerate {
    name: "summary-default",
    reply: `"Venue and catering are booked."`,
    attributes: {},
  },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s18_SummarizeHead_request(head, provider = provider))
}

test "18: summarize-and-replace keeps the recent tail verbatim" {
  let history = [
    root.ai.ChatMessage.user("plan the offsite"),
    root.ai.ChatMessage.assistant("venue booked"),
    root.ai.ChatMessage.user("what is left?"),
  ];
  let compacted = s18_compact_history(history, 1, root.ai.FixtureGenerate {
    name: "summary",
    reply: `"The venue is booked."`,
    attributes: {},
  });
  assert.equal(compacted.length(), 2)
  assert.equal((compacted.at(1) ?? root.ai.ChatMessage.user("missing")).text(), "what is left?")
}

test "18: short history is returned without a model call" {
  let history = [root.ai.ChatMessage.user("hi")];
  assert.equal(s18_compact_history(history, 2, root.ai.fixture(`"unused"`)), history)
}

testset "integ-test-18-compaction" {
  test "18 live: summarize-and-replace over real OpenAI compaction" {
    let history: root.ai.ChatMessage[] = [
      root.ai.ChatMessage.user("We are debugging a refund bug. Reproduced on staging."),
      root.ai.ChatMessage.assistant("Root cause: the refund webhook double-fires."),
      root.ai.ChatMessage.user("We added an idempotency key. Fixed."),
      root.ai.ChatMessage.assistant("Confirmed no duplicate refunds after the fix."),
      root.ai.ChatMessage.user("Now: what's the next task?"),
    ];
    let compacted = s18_compact_history(history, 1, root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
    });
    assert.equal(compacted.length(), 2)
    assert.equal((compacted.at(1) ?? root.ai.ChatMessage.user("missing")).text(), "Now: what's the next task?")
    assert.is_true((compacted.at(0) ?? root.ai.ChatMessage.user("missing")).text().length() > 0)
  }
}
