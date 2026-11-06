class Search { tool: "search", query: string, limit: int? }
class Calc { tool: "calc", op: "add" | "sub" | "mul", a: int, b: int }
class Finish { tool: "finish", answer: string, confidence: "low" | "medium" | "high" }
type Action = Search | Calc | Finish
function main() -> int {
  let action = baml.json.decode<Action>("{\"tool\":\"browse\"}");
  0
}
