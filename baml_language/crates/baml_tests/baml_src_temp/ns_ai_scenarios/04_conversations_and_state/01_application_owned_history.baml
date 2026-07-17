test "guide 04.01: editable conversation history is ordinary application data" {
  let history = sample_conversation();
  let updated = history.append(root.ai.ChatMessage.user("Order O-42."));
  assert.equal(history.items().length(), 2)
  assert.equal(updated.items().length(), 3)
}

testset "integ-test-guide-04-application-history" {
  test "guide 04.01 live OpenAI: edited application history becomes model input" {
    let history = sample_conversation();
    let updated = match (history.append(root.ai.ChatMessage.user("The order is O-42."))) {
      let conversation: root.ai.Conversation => conversation,
      _ => baml.sys.panic("expected Conversation"),
    };
    assert.equal(history.entries.length(), 2)
    assert.equal(updated.entries.length(), 3)
    let response = root.ai.drivers.generate_with_meta<ConversationProbe>(
      ConversationProbe_task(updated, live_openai()),
    );
    log.info("live application history " + {
      "original_messages": history.entries.length(),
      "updated_messages": updated.entries.length(),
      "evidence": response.value.evidence,
    }.to_string());
    assert.contains(response.value.evidence, "O-42")
  }
}
