// Shorten a conversation while preserving the important fact needed by the
// next model call.
function compaction_and_memory() -> ConversationProbe {
  let source = sample_history();
  let compacted = compact_history(source);
  let task = ConversationProbe_task(
    compacted,
    root.ai.testing.fake_output_provider(`{
      "branch": "general",
      "evidence": "Summary: customer reports a duplicate charge; support acknowledged it."
    }`),
  );
  let result = task.run(
    runner = root.ai.run.Generation<ConversationProbe>.new(),
  );
  log.info({
    "source_messages": source.entries.length(),
    "compacted_messages": compacted.entries.length(),
    "compacted_history": history_text(compacted),
    "result": result,
  });
  result
}
