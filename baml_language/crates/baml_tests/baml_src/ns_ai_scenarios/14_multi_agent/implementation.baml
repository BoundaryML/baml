class S14TransferArgs {
  reason: string,
}

function s14_billing_handoff_tool() -> root.ai.Tool throws baml.errors.Unsupported {
  let tool = root.ai.Tool.from_type(
    "transfer_to_billing",
    "Transfer billing, refund, and charge questions.",
    reflect.type_of<S14TransferArgs>(),
  );
  root.ai.Tool {
    name: tool.name,
    description: tool.description,
    parameters: tool.parameters,
    handoff: true,
  }
}

function s14_noop_dispatch(calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never { [] }

function s14_route(
  question: string,
  triage: root.ai.Provider,
  specialists: map<string, root.ai.Provider>,
) -> string throws baml.errors.ToolError | baml.errors.UnknownError {
  let first = s14_Triage_agent_manual(
    question,
    triage,
    [s14_billing_handoff_tool()],
    s14_noop_dispatch,
  );
  match (first) {
    let done: root.ai.Done<string> => done.value,
    let ignored: root.ai.BudgetReached => "triage budget reached",
    let handoff: root.ai.Handoff => match (specialists.get(handoff.to)) {
      null => "no specialist for " + handoff.to,
      let specialist: root.ai.Provider => match (s14_Triage_agent_manual(
        question,
        specialist,
        [],
        s14_noop_dispatch,
      )) {
        let done: root.ai.Done<string> => done.value,
        let ignored: root.ai.BudgetReached => "specialist budget reached",
        let next: root.ai.Handoff => "nested handoff to " + next.to,
      },
    },
  }
}
