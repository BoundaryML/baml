test "guide 04.03: two conversation branches can diverge independently" {
  let base = sample_conversation();
  let billing = base.append(root.ai.ChatMessage.user("Please refund it."));
  let fraud = base.append(root.ai.ChatMessage.user("Could this be fraud?"));
  assert.contains(root.ai.message_text(billing.items()[2]), "refund")
  assert.contains(root.ai.message_text(fraud.items()[2]), "fraud")
}

