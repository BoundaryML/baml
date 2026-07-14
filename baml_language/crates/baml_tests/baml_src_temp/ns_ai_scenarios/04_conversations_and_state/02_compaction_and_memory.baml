test "guide 04.02: compaction creates a lossy conversation projection" {
  let compacted = compact_conversation(sample_conversation());
  assert.equal(compacted.entries.length(), 1)
  assert.contains(root.ai.message_text(compacted.entries[0]), "duplicate charge")
}

