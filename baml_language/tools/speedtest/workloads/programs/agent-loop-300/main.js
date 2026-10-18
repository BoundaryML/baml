const isInt = v => typeof v === "number" && Number.isInteger(v);
class AgentFailed { constructor(retries) { this.retries = retries; } }
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
class Rng {
  constructor(seed) { this.seed = seed; }
  draw(n) { this.seed = this.seed * 16807 % 2147483647; return this.seed % n; }
}
const WORDS = ["weather", "paris", "capital", "population", "rust", "python", "llm", "agent", "tokens", "latency"];
function conversations(count) {
  const rng = new Rng(17);
  const convs = [];
  for (let c = 0; c < count; c++) {
    const turns = [];
    const n = 2 + rng.draw(6);
    for (let t = 0; t < n; t++) {
      const kind = rng.draw(10);
      if (kind < 4) {
        const q = [];
        const m = 1 + rng.draw(3);
        for (let k = 0; k < m; k++) q.push(WORDS[rng.draw(10)]);
        let r = `{"tool": "search", "query": "${q.join(" ")}"`;
        if (rng.draw(2) === 1) r += `, "limit": ${1 + rng.draw(9)}`;
        turns.push(r + "}");
      } else if (kind < 7) {
        const op = ["add", "sub", "mul"][rng.draw(3)];
        const a = rng.draw(1000), b = rng.draw(1000);
        turns.push(`{"tool": "calc", "op": "${op}", "a": ${a}, "b": ${b}}`);
      } else if (kind === 7) {
        turns.push('{"tool": "search", "query": "broken"}'.slice(0, 5 + rng.draw(10)));
      } else if (kind === 8) {
        turns.push('{"tool": "browse", "url": "http://x"}');
      } else {
        turns.push(`{"tool": "calc", "op": "add", "a": ${rng.draw(10)}}`);
      }
    }
    const answer = rng.draw(100000);
    turns.push(`{"tool": "finish", "answer": "answer ${answer}", "confidence": "${["low", "medium", "high"][rng.draw(3)]}"}`);
    convs.push(turns);
  }
  return convs;
}

function main() {
  const convs = conversations(300);
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
  return check;
}
