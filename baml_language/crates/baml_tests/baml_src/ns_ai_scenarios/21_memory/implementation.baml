// 21 — long-term memory is application data unless a provider truly owns it.
class s21_MemoryItem {
  subject: string,
  fact: string,
}

class s21_Memories {
  items: s21_MemoryItem[],
}

function s21_render_memories(items: s21_MemoryItem[]) -> string throws never {
  if (items.length() == 0) { return "(no memories yet)"; }
  items.map((m) -> { `- ${m.subject}: ${m.fact}` }).join("\n")
}
