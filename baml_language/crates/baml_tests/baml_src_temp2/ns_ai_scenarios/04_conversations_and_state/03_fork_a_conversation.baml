// Explore two independent follow-up paths from the same conversation without
// either branch changing the other.
function fork_a_conversation() -> ConversationProbe[] {
  let base = sample_history();
  let billing = base
    .append(root.ai.ChatMessage.user("Please refund it."))
    .to_history();
  let fraud = base
    .append(root.ai.ChatMessage.user("Could this be fraud?"))
    .to_history();

  let billing_result = ConversationProbe_task(
    billing,
    root.ai.testing.fake_output_provider(`{
      "branch": "billing",
      "evidence": "Please refund it."
    }`),
  ).run(runner = root.ai.run.Generation<ConversationProbe>.new());
  let fraud_result = ConversationProbe_task(
    fraud,
    root.ai.testing.fake_output_provider(`{
      "branch": "fraud",
      "evidence": "Could this be fraud?"
    }`),
  ).run(runner = root.ai.run.Generation<ConversationProbe>.new());

  let results = [billing_result, fraud_result];
  log.info({
    "base_messages": base.entries.length(),
    "billing_messages": billing.entries.length(),
    "fraud_messages": fraud.entries.length(),
    "results": results,
  });
  results
}
