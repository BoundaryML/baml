test "guide 04.01: editable conversation history is ordinary application data" {
  let history = sample_conversation();
  let updated = history.append(root.ai.ChatMessage.user("Order O-42."));
  assert.equal(history.items().length(), 2)
  assert.equal(updated.items().length(), 3)
}

