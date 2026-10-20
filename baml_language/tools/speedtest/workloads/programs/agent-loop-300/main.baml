// An agent loop over recorded model responses, the shape of a BAML program
// around its LLM calls with the network removed: each response is decoded with
// typed JSON deserialize into a union of tool-call classes (string-literal
// discriminants, an optional field, a literal-union field), dispatched with
// match, and folded into per-conversation state (a map scratchpad and a message
// history). Malformed responses (truncated JSON, unknown tool, missing field)
// throw, are caught and retried; too many retries abandon the conversation
// with a custom error class. 300 conversations, 3 passes.
class Search { tool: "search", query: string, limit: int? }
class Calc { tool: "calc", op: "add" | "sub" | "mul", a: int, b: int }
class Finish { tool: "finish", answer: string, confidence: "low" | "medium" | "high" }
type Action = Search | Calc | Finish
class AgentState {
  scratch: map<string, int>
  history: string[]
  steps: int
  retries: int
  answer: string?
}
class AgentFailed { retries: int }
function fake_search(query: string, limit: int) -> int {
  let h = 0;
  for (let c in query.to_code_points()) { h = (h * 31 + c) % 1000003; }
  h % (limit * 100)
}
function step(state: AgentState, action: Action) -> bool {
  state.steps += 1;
  match (action) {
    Search => {
      let value = fake_search(action.query, action.limit ?? 10);
      state.scratch[action.query] = value;
      state.history.push("search:" + action.query + "=" + value.to_string());
      false
    },
    Calc => {
      let value = match (action.op) {
        "add" => action.a + action.b,
        "sub" => action.a - action.b,
        "mul" => action.a * action.b,
      };
      state.scratch["calc" + state.steps.to_string()] = value;
      state.history.push("calc:" + action.op + "=" + value.to_string());
      false
    },
    Finish => {
      state.answer = action.answer + "/" + action.confidence;
      state.history.push("finish");
      true
    },
  }
}
function run_conversation(turns: string[]) -> AgentState {
  let state = AgentState { scratch: {}, history: [], steps: 0, retries: 0, answer: null };
  for (let raw in turns) {
    let action: Action? = baml.json.deserialize<Action>(raw) catch (_) { let _ => null };
    if (action == null) {
      state.retries += 1;
      state.history.push("retry");
      if (state.retries > 2) { throw AgentFailed { retries: state.retries }; }
    } else {
      if (step(state, action)) { break; }
    }
  }
  state
}
// The recorded responses: 300 conversations of 2-7 tool calls ending in a
// finish, with truncated JSON, unknown tools and missing fields mixed in,
// built from a seeded LCG.
class Rng {
  seed: int
}

function draw(rng: Rng, n: int) -> int {
  rng.seed = rng.seed * 16807 % 2147483647;
  rng.seed % n
}

function conversations(count: int) -> string[][] {
  let rng = Rng { seed: 17 };
  let words = ["weather", "paris", "capital", "population", "rust", "python", "llm", "agent", "tokens", "latency"];
  let ops = ["add", "sub", "mul"];
  let levels = ["low", "medium", "high"];
  let convs: string[][] = [];
  for (let c = 0; c < count; c += 1) {
    let turns: string[] = [];
    let n = 2 + draw(rng, 6);
    for (let t = 0; t < n; t += 1) {
      let kind = draw(rng, 10);
      if (kind < 4) {
        let q: string[] = [];
        let m = 1 + draw(rng, 3);
        for (let k = 0; k < m; k += 1) { q.push(words[draw(rng, 10)]); }
        let r = "{\"tool\": \"search\", \"query\": \"" + q.join(" ") + "\"";
        if (draw(rng, 2) == 1) { r = r + ", \"limit\": " + (1 + draw(rng, 9)).to_string(); }
        turns.push(r + "}");
      } else if (kind < 7) {
        let op = ops[draw(rng, 3)];
        let a = draw(rng, 1000);
        let b = draw(rng, 1000);
        turns.push("{\"tool\": \"calc\", \"op\": \"" + op + "\", \"a\": " + a.to_string() + ", \"b\": " + b.to_string() + "}");
      } else if (kind == 7) {
        turns.push("{\"tool\": \"search\", \"query\": \"broken\"}".slice(0, 5 + draw(rng, 10)));
      } else if (kind == 8) {
        turns.push("{\"tool\": \"browse\", \"url\": \"http://x\"}");
      } else {
        turns.push("{\"tool\": \"calc\", \"op\": \"add\", \"a\": " + draw(rng, 10).to_string() + "}");
      }
    }
    let answer = draw(rng, 100000);
    turns.push("{\"tool\": \"finish\", \"answer\": \"answer " + answer.to_string() + "\", \"confidence\": \"" + levels[draw(rng, 3)] + "\"}");
    convs.push(turns);
  }
  convs
}

function main() -> int {
  let convs = conversations(300);
  let check = 0;
  for (let pass = 0; pass < 3; pass += 1) {
    for (let turns in convs) {
      let score = 0;
      let state = run_conversation(turns) catch (e) {
        AgentFailed => { score = 7 + e.retries; null }
      };
      if (state != null) {
        let total = 0;
        for (let v in state.scratch.values()) { total += v; }
        let answer = state.answer ?? "";
        score = state.steps * 1000 + state.retries * 100 + answer.length() + state.history.length() * 3 + (total % 9973 + 9973) % 9973;
      }
      check = (check * 31 + score) % 1000000007;
    }
  }
  check
}
