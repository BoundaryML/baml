test "guide 04.03: two conversation branches can diverge independently" {
  let base = sample_conversation();
  let billing = base.append(root.ai.ChatMessage.user("Please refund it."));
  let fraud = base.append(root.ai.ChatMessage.user("Could this be fraud?"));
  assert.contains(root.ai.message_text(billing.items()[2]), "refund")
  assert.contains(root.ai.message_text(fraud.items()[2]), "fraud")
}

testset "integ-test-guide-04-fork-conversation" {
  test "guide 04.03 live OpenAI: independent branches produce branch-specific results" {
    let base = sample_conversation();
    let billing = match (base.append(root.ai.ChatMessage.user("Please refund it."))) {
      let conversation: root.ai.Conversation => conversation,
      _ => baml.sys.panic("expected billing Conversation"),
    };
    let fraud = match (base.append(root.ai.ChatMessage.user("Could this be fraud?"))) {
      let conversation: root.ai.Conversation => conversation,
      _ => baml.sys.panic("expected fraud Conversation"),
    };
    let provider = live_openai();
    let billing_task = ConversationProbe_task(billing, provider);
    let fraud_task = ConversationProbe_task(fraud, provider);
    let billing_result = billing_task.drive(root.ai.driver.generate<ConversationProbe>());
    let fraud_result = fraud_task.drive(root.ai.driver.generate<ConversationProbe>());
    log.info("live conversation forks " + {
      "base_messages": base.entries.length(),
      "billing": billing_result,
      "fraud": fraud_result,
    }.to_string());
    assert.equal(base.entries.length(), 2)
    assert.equal(billing_result.branch, "billing")
    assert.equal(fraud_result.branch, "fraud")
  }
}
