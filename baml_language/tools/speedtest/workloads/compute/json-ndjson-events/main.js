class Rng {
  constructor(seed) { this.seed = seed; }
  draw(n) { this.seed = this.seed * 16807 % 2147483647; return this.seed % n; }
}

function setup() {
  const rng = new Rng(23);
  const words = ["agent", "tool", "result", "latency", "token", "stream", "retry", "model", "prompt", "cache", "session", "vector"];
  const types = ["message", "tool_call", "tool_result", "status"];
  const roles = ["user", "assistant", "system"];
  const tools = ["search", "calculator", "browser", "shell"];
  const lines = [];
  for (let i = 0; i < 2000; i++) {
    const kind = rng.draw(4);
    const parts = [];
    let length = 0;
    while (length < 290) {
      const word = words[rng.draw(12)];
      parts.push(word);
      length += word.length + 1;
    }
    let content = "\"" + parts.join(" ") + "\"";
    if (rng.draw(3) === 0) content = "\"" + parts.join(" ") + "\\n\\\"done\\\"\"";
    if (kind === 3) content = "null";
    const role = kind === 0 ? "\"" + roles[rng.draw(3)] + "\"" : "null";
    let line = "{\"id\":\"evt_" + i + "_" + rng.draw(1000000) + "\"";
    line += ",\"session_id\":\"sess_" + rng.draw(50) + "\"";
    line += ",\"event_type\":\"" + types[kind] + "\"";
    line += ",\"role\":" + role;
    line += ",\"content\":" + content;
    if (kind === 1 || kind === 2) line += ",\"tool_name\":\"" + tools[rng.draw(4)] + "\"";
    line += ",\"extra\":{\"a\":[1,2,3],\"b\":\"z\"}";
    line += ",\"created_at\":" + (1700000000000 + i * 1000 + rng.draw(1000)) + "}";
    lines.push(line);
  }
  return lines;
}

function run(lines) {
  let check = 0;
  for (const line of lines) {
    const e = JSON.parse(line);
    let h = e.created_at % 100000 + e.id.length + e.session_id.length + e.event_type.length;
    h += (e.role ?? "x").length + (e.content ?? "nu").length + (e.tool_name ?? "abc").length;
    check = (check + h) % 1000000007;
  }
  return check;
}

function main() { return run(setup()); }
