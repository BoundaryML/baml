function s18_compact_history(
  history: root.ai.ChatMessage[],
  keep_last: int,
  provider: root.ai.Provider,
) -> root.ai.ChatMessage[] throws baml.errors.CallError | baml.errors.UnknownError {
  let n = history.length();
  if (n <= keep_last) { return history; }
  let head = history.slice(0, n - keep_last);
  let tail = history.slice(n - keep_last, n);
  let synopsis = s18_SummarizeHead_manual(root.ai.render_messages(head), provider = provider);
  [root.ai.ChatMessage.system("Summary of earlier conversation: " + synopsis)].concat(tail)
}
