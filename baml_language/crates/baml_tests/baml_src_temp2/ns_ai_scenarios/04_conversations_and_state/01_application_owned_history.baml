// Let the application edit conversation history directly, then ask the model to
// reason over the updated conversation.
function application_owned_history() -> ConversationProbe {
  let original = sample_history();
  let updated = original
    .append(root.ai.ChatMessage.user("The order is O-42."))
    .to_history();
  let task = ConversationProbe_task(
    updated,
    root.ai.fake_output_provider(`{
      "branch": "general",
      "evidence": "The order is O-42."
    }`),
  );
  let result = task.run(
    runner = root.ai.run.Generation<ConversationProbe>.new(),
  );
  log.info({
    "original_messages": original.entries.length(),
    "updated_messages": updated.entries.length(),
    "history": history_text(updated),
    "result": result,
  });
  result
}
