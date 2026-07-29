// Save exact provider conversation state as an opaque token and restore it for
// a later run.
function save_and_resume(
) -> root.ai.ConversationToken throws root.ai.Failure
    | baml.errors.UnknownError
    | baml.errors.Unsupported
{
  let provider = root.openai.Chat {
    ...root.openai.chat(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let task = ResolveTicketWithTools_task(
    sample_ticket(),
    provider,
    [search_knowledge],
  );
  let pause_before_request = root.ai.Budget {
    max_steps: 0,
    max_cost_usd: null,
  };

  let paused = task.run(
    runner = root.ai.run.Agent<Resolution>.new(budget = pause_before_request),
  );
  let conversation = match (paused) {
    let done: root.ai.Done<Resolution> => done.conversation,
    let stopped: root.ai.BudgetReached => stopped.conversation,
    let handoff: root.ai.Handoff => handoff.conversation,
  };
  let token = provider.save_conversation(conversation);
  let restored = provider.restore_conversation(token);

  let resumed = task.run(
    runner = root.ai.run.Agent<Resolution>.new(
      budget = pause_before_request,
      conversation = restored,
    ),
  );
  let resumed_provider = match (resumed) {
    let done: root.ai.Done<Resolution> => done.conversation.provider().name(),
    let stopped: root.ai.BudgetReached => stopped.conversation.provider().name(),
    let handoff: root.ai.Handoff => handoff.conversation.provider().name(),
  };
  log.info({
    "saved_provider": token.provider,
    "saved_bytes": token.sealed.length(),
    "resumed_provider": resumed_provider,
  });
  token
}
