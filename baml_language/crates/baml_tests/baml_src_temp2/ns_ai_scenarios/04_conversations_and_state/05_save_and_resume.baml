// Save exact provider conversation state as an opaque token and restore it for
// a later run.
function save_and_resume(
) -> ai.ConversationToken throws ai.Failure
    | baml.errors.UnknownError
    | baml.errors.Unsupported
{
  let provider = root.openai.OpenAIProvider {
    ...root.openai.responses(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
  let pause_before_request = ai.Budget {
    max_steps: 0,
    max_cost_usd: null,
  };

  let paused = task.run(
    runner = ai.run.Agent<Resolution>.new(budget = pause_before_request),
  );
  let conversation = match (paused) {
    let done: ai.Done<Resolution> => done.conversation,
    let stopped: ai.BudgetReached => stopped.conversation,
    let handoff: ai.Handoff => handoff.conversation,
    let interrupted: ai.Interrupted => interrupted.conversation,
  };
  let token = provider.save_conversation(conversation);
  let restored = provider.restore_conversation(token);

  let resumed = task.run(
    runner = ai.run.Agent<Resolution>.new(
      budget = pause_before_request,
      conversation = restored,
    ),
  );
  let resumed_provider = match (resumed) {
    let done: ai.Done<Resolution> => done.conversation.provider().name(),
    let stopped: ai.BudgetReached => stopped.conversation.provider().name(),
    let handoff: ai.Handoff => handoff.conversation.provider().name(),
    let interrupted: ai.Interrupted => interrupted.conversation.provider().name(),
  };
  log.info({
    "saved_provider": token.provider,
    "saved_bytes": token.sealed.length(),
    "resumed_provider": resumed_provider,
  });
  token
}
