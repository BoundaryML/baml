# programs::agent loop 300

## eval-setup
```py
import json
def rnd(n, state=[17]):
    state[0] = state[0] * 16807 % 2147483647
    return state[0] % n
words = ["weather", "paris", "capital", "population", "rust", "python", "llm", "agent", "tokens", "latency"]
convs = []
for c in range(300):
    turns = []
    for t in range(2 + rnd(6)):
        kind = rnd(10)
        if kind < 4:
            q = []
            for _ in range(1 + rnd(3)):
                q.append(words[rnd(10)])
            r = {"tool": "search", "query": " ".join(q)}
            if rnd(2): r["limit"] = 1 + rnd(9)
            turns.append(json.dumps(r))
        elif kind < 7:
            turns.append(json.dumps({"tool": "calc", "op": ["add", "sub", "mul"][rnd(3)], "a": rnd(1000), "b": rnd(1000)}))
        elif kind == 7:
            turns.append(json.dumps({"tool": "search", "query": "broken"})[: 5 + rnd(10)])
        elif kind == 8:
            turns.append(json.dumps({"tool": "browse", "url": "http://x"}))
        else:
            turns.append(json.dumps({"tool": "calc", "op": "add", "a": rnd(10)}))
    turns.append(json.dumps({"tool": "finish", "answer": "answer %d" % rnd(100000), "confidence": ["low", "medium", "high"][rnd(3)]}))
    convs.append(turns)
raw = json.dumps(convs)
baml_raw = json.dumps(raw)
py_raw = repr(raw)
js_raw = json.dumps(raw)
```

## BAML
```baml
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
function main() -> int {
  let convs = baml.json.deserialize<string[][]>($$baml_raw);
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
```

## Python
```python
import json
class AgentFailed(Exception):
    def __init__(self, retries): self.retries = retries
def is_int(v): return type(v) is int
def decode(raw):
    try:
        d = json.loads(raw)
    except ValueError:
        return None
    if not isinstance(d, dict): return None
    t = d.get("tool")
    if t == "search" and isinstance(d.get("query"), str) and (d.get("limit") is None or is_int(d["limit"])):
        return d
    if t == "calc" and d.get("op") in ("add", "sub", "mul") and is_int(d.get("a")) and is_int(d.get("b")):
        return d
    if t == "finish" and isinstance(d.get("answer"), str) and d.get("confidence") in ("low", "medium", "high"):
        return d
    return None
def fake_search(query, limit):
    h = 0
    for ch in query: h = (h * 31 + ord(ch)) % 1000003
    return h % (limit * 100)
def run_conversation(turns):
    st = {"scratch": {}, "history": [], "steps": 0, "retries": 0, "answer": None}
    for raw in turns:
        a = decode(raw)
        if a is None:
            st["retries"] += 1
            st["history"].append("retry")
            if st["retries"] > 2: raise AgentFailed(st["retries"])
            continue
        st["steps"] += 1
        t = a["tool"]
        if t == "search":
            lim = a.get("limit")
            v = fake_search(a["query"], 10 if lim is None else lim)
            st["scratch"][a["query"]] = v
            st["history"].append("search:" + a["query"] + "=" + str(v))
        elif t == "calc":
            op = a["op"]
            v = a["a"] + a["b"] if op == "add" else a["a"] - a["b"] if op == "sub" else a["a"] * a["b"]
            st["scratch"]["calc" + str(st["steps"])] = v
            st["history"].append("calc:" + op + "=" + str(v))
        else:
            st["answer"] = a["answer"] + "/" + a["confidence"]
            st["history"].append("finish")
            break
    return st
convs = json.loads($$py_raw)
check = 0
for _ in range(3):
    for turns in convs:
        try:
            st = run_conversation(turns)
            score = (st["steps"] * 1000 + st["retries"] * 100 + len(st["answer"] or "")
                     + len(st["history"]) * 3 + sum(st["scratch"].values()) % 9973)
        except AgentFailed as e:
            score = 7 + e.retries
        check = (check * 31 + score) % 1000000007
print(check)
```

## Typescript
```ts
class AgentFailed { constructor(retries) { this.retries = retries; } }
const isInt = v => typeof v === "number" && Number.isInteger(v);
function decode(raw) {
  let d;
  try { d = JSON.parse(raw); } catch (e) { return null; }
  if (d === null || typeof d !== "object" || Array.isArray(d)) return null;
  const t = d.tool;
  if (t === "search" && typeof d.query === "string" && (d.limit == null || isInt(d.limit))) return d;
  if (t === "calc" && ["add", "sub", "mul"].includes(d.op) && isInt(d.a) && isInt(d.b)) return d;
  if (t === "finish" && typeof d.answer === "string" && ["low", "medium", "high"].includes(d.confidence)) return d;
  return null;
}
function fakeSearch(query, limit) {
  let h = 0;
  for (let i = 0; i < query.length; i++) h = (h * 31 + query.charCodeAt(i)) % 1000003;
  return h % (limit * 100);
}
function runConversation(turns) {
  const st = { scratch: new Map(), history: [], steps: 0, retries: 0, answer: null };
  for (const raw of turns) {
    const a = decode(raw);
    if (a === null) {
      st.retries++;
      st.history.push("retry");
      if (st.retries > 2) throw new AgentFailed(st.retries);
      continue;
    }
    st.steps++;
    if (a.tool === "search") {
      const v = fakeSearch(a.query, a.limit ?? 10);
      st.scratch.set(a.query, v);
      st.history.push("search:" + a.query + "=" + v);
    } else if (a.tool === "calc") {
      const v = a.op === "add" ? a.a + a.b : a.op === "sub" ? a.a - a.b : a.a * a.b;
      st.scratch.set("calc" + st.steps, v);
      st.history.push("calc:" + a.op + "=" + v);
    } else {
      st.answer = a.answer + "/" + a.confidence;
      st.history.push("finish");
      break;
    }
  }
  return st;
}
const convs = JSON.parse($$js_raw);
let check = 0;
for (let pass = 0; pass < 3; pass++) {
  for (const turns of convs) {
    let score;
    try {
      const st = runConversation(turns);
      let total = 0;
      for (const v of st.scratch.values()) total += v;
      score = st.steps * 1000 + st.retries * 100 + (st.answer ?? "").length + st.history.length * 3 + (total % 9973 + 9973) % 9973;
    } catch (e) {
      if (!(e instanceof AgentFailed)) throw e;
      score = 7 + e.retries;
    }
    check = (check * 31 + score) % 1000000007;
  }
}
console.log(check);
```
