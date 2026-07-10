function s18_text_of(message: root.ai.ChatMessage) -> string throws never {
  let out = "";
  for (let part in message.parts) { out = out + (part.text ?? ""); }
  out
}

function s18_render_head(history: root.ai.ChatMessage[]) -> string throws never {
  history.map((m: root.ai.ChatMessage) -> string {
    m.role + ": " + s18_text_of(m)
  }).join("\n")
}

function s18_compact_history(
  history: root.ai.ChatMessage[],
  keep_last: int,
  provider: root.ai.Provider,
) -> root.ai.ChatMessage[] throws baml.errors.CallError | baml.errors.UnknownError {
  let n = history.length();
  if (n <= keep_last) { return history; }
  let head = history.slice(0, n - keep_last);
  let tail = history.slice(n - keep_last, n);
  let synopsis = s18_SummarizeHead_manual(s18_render_head(head), provider = provider);
  [root.ai.ChatMessage.system("Summary of earlier conversation: " + synopsis)].concat(tail)
}
