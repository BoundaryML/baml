// Move an in-progress tool conversation to another provider between model turns
// without restarting the workflow.
function switch_provider_between_turns() -> null {
  let first = fake_tool_provider([one_knowledge_call()]);
  let second = root.ai.FakeToolProvider {
    ...fake_tool_provider([]),
    provider_name: "fake-tools-secondary",
  };
  let trace = guide_agent_trace();
  let task = ResolveTicketWithTools_task(
    sample_ticket(),
    first,
    [search_knowledge],
  );
  let outcome = task.run(
    runner = root.ai.run.Agent<Resolution>.new(
      tools = [search_knowledge],
      prepare_step = (context) -> {
        root.ai.StepPlan {
          provider: if (context.step == 1) { second } else { null },
          tools: null,
          stop: null,
        }
      },
      on_event = (event) -> {
        trace.events.push(event.kind());
        null
      },
    ),
  );
  log.info({
    "scenario": "switch_provider_between_turns",
    "events": trace.events,
    "outcome": outcome,
  });
  null
}
