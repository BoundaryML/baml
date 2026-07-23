// 14 — a Handoff outcome is routed by ordinary application code.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s14_Triage(question: string) -> string {
//   client: root.ai.FakeToolProvider {
//     call_name: "transfer_to_billing",
//     call_args: `{"reason":"duplicate charge"}`,
//     final_json: `"unused"`,
//   }
//   prompt: `Triage this customer question: ${question}`
// }

// Manual expansion reference for s14_Triage.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s14_Triage_request(...) models s14_Triage.request(...) with its call-time
//   handoff roster and dispatcher.
// - s14_Triage_agent_manual(...) models s14_Triage.agent(...):
//   root.ai.run_agent(request), preserving Done | BudgetReached | Handoff.
// Handoff routing in s14_route is ordinary application code, not a task modifier.
// This scenario intentionally has no plain-call s14_Triage_manual(...) wrapper.
function s14_Triage_request(
  question: string,
  provider: root.ai.Provider = root.ai.FakeToolProvider {
    call_name: "transfer_to_billing",
    call_args: `{"reason":"duplicate charge"}`,
    final_json: `"unused"`,
  },
  tools: root.ai.Tool[] = [],
  dispatch: ((root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never)? = null,
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s14_Triage",
    "ns_ai_scenarios",
    { "question": question },
    tools,
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    root.ai.prompt_template(`Triage this customer question: ${question}`, false),
  )
}

function s14_Triage_agent_manual(
  question: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<string> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<string>(s14_Triage_request(
    question,
    provider = provider,
    tools = tools,
    dispatch = dispatch,
  ))
}

test "14: application code routes a typed handoff to a specialist" {
  let answer = s14_route(
    "I was charged twice.",
    root.ai.FakeToolProvider {
      call_name: "transfer_to_billing",
      call_args: `{"reason":"duplicate charge"}`,
      final_json: `"unused"`,
    },
    {
      "transfer_to_billing": root.ai.FakeToolProvider {
        call_name: null,
        call_args: "{}",
        final_json: `"Your duplicate charge was refunded."`,
      },
    },
  );
  assert.is_true(answer.to_lower_case().includes("refund"))
}

test "14: an unknown target stays explicit" {
  let answer = s14_route(
    "I was charged twice.",
    root.ai.FakeToolProvider {
      call_name: "transfer_to_billing",
      call_args: `{"reason":"duplicate charge"}`,
      final_json: `"unused"`,
    },
    {},
  );
  assert.is_true(answer.includes("no specialist"))
}

testset "integ-test-14-multi-agent" {
  test "14 live: a real triage model transfers to the specialist" {
    let answer = s14_route(
      "I was charged twice for my subscription.",
      root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
      {
        "transfer_to_billing": root.ai.FakeToolProvider {
          call_name: null,
          call_args: "{}",
          final_json: `"Your duplicate charge has been refunded."`,
        },
      },
    );
    assert.is_true(answer.to_lower_case().includes("refund"))
  }
}
