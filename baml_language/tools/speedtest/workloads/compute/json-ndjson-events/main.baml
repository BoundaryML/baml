// Decode an NDJSON event log line by line into a typed class (B-1815): 2,000
// lines of about 400 bytes, each with optional fields that are null or absent
// and an `extra` object the class does not declare. The lines are built once
// in setup(); a job decodes every line with `baml.json.decode<Event>` and folds
// the fields into a checksum.
class Event {
  id: string
  session_id: string
  event_type: string
  role: string?
  content: string?
  tool_name: string?
  created_at: int
}

class Rng {
  seed: int
}

function draw(rng: Rng, n: int) -> int {
  rng.seed = rng.seed * 16807 % 2147483647;
  rng.seed % n
}

function setup() -> string[] {
  let rng = Rng { seed: 23 };
  let words = ["agent", "tool", "result", "latency", "token", "stream", "retry", "model", "prompt", "cache", "session", "vector"];
  let types = ["message", "tool_call", "tool_result", "status"];
  let roles = ["user", "assistant", "system"];
  let tools = ["search", "calculator", "browser", "shell"];
  let lines: string[] = [];
  for (let i = 0; i < 2000; i += 1) {
    let kind = draw(rng, 4);
    let parts: string[] = [];
    let length = 0;
    while (length < 290) {
      let word = words[draw(rng, 12)];
      parts.push(word);
      length += word.length() + 1;
    }
    let content = "\"" + parts.join(" ") + "\"";
    if (draw(rng, 3) == 0) { content = "\"" + parts.join(" ") + "\\n\\\"done\\\"\""; }
    if (kind == 3) { content = "null"; }
    let role = if (kind == 0) { "\"" + roles[draw(rng, 3)] + "\"" } else { "null" };
    let line = "{\"id\":\"evt_" + i.to_string() + "_" + draw(rng, 1000000).to_string() + "\"";
    line = line + ",\"session_id\":\"sess_" + draw(rng, 50).to_string() + "\"";
    line = line + ",\"event_type\":\"" + types[kind] + "\"";
    line = line + ",\"role\":" + role;
    line = line + ",\"content\":" + content;
    if (kind == 1 || kind == 2) { line = line + ",\"tool_name\":\"" + tools[draw(rng, 4)] + "\""; }
    line = line + ",\"extra\":{\"a\":[1,2,3],\"b\":\"z\"}";
    line = line + ",\"created_at\":" + (1700000000000 + i * 1000 + draw(rng, 1000)).to_string() + "}";
    lines.push(line);
  }
  lines
}

function run(lines: string[]) -> int {
  let check = 0;
  for (let line in lines) {
    let e = baml.json.decode<Event>(line);
    let h = e.created_at % 100000 + e.id.length() + e.session_id.length() + e.event_type.length();
    h += (e.role ?? "x").length() + (e.content ?? "nu").length() + (e.tool_name ?? "abc").length();
    check = (check + h) % 1000000007;
  }
  check
}

function main() -> int {
  run(setup())
}
