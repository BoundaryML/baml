// Save an agent session as a durable token and restore it later — in this
// process or another one. The provider serializes the conversation half; the
// application re-supplies the task half; the token verifies the pairing.
function save_and_resume(
) -> ai.run.AgentSessionToken throws ai.Failure
    | baml.errors.UnknownError
    | baml.errors.Unsupported
{
  let provider = root.openai.OpenAIProvider {
    ...root.openai.responses(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);

  //# Pause before the first request (max_steps = 0) so this example needs no network
  let paused = task.run(
    runner = ai.run.Agent<Resolution>.new(max_steps = 0),
  );
  let session = ai.run.AgentSession<Resolution>.of(task, paused);

  //# The token is small, JSON-serializable, and names its task
  let token = session.save();

  //# A fresh process re-supplies the same task; restore verifies the pairing
  //# before any model request happens
  let restored = ai.run.AgentSession<Resolution>.restore(task, token);
  //# resume advances `restored` in place; keep using the same session
  let resumed = restored.resume(
    runner = ai.run.Agent<Resolution>.new(max_steps = 0),
  );

  log.info({
    "saved_task": token.task_identity,
    "saved_provider": token.conversation.provider,
    "saved_bytes": token.conversation.sealed.length(),
    "resumed_provider": restored.conversation().provider().name(),
  });
  token
}
