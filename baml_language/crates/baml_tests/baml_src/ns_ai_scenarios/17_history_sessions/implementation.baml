// Application-owned history is ordinary serializable data, not a Session.
function s17_text_of(message: root.ai.ChatMessage) -> string throws never {
  let out = "";
  for (let part in message.parts) { out = out + (part.text ?? ""); }
  out
}

function s17_render_history(history: root.ai.ChatMessage[]) -> string throws never {
  history.map((m: root.ai.ChatMessage) -> string {
    m.role + ": " + s17_text_of(m)
  }).join("\n")
}
