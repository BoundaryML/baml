const ESC = { 34: '"', 92: '\\', 47: '/', 98: '\b', 102: '\f', 110: '\n', 114: '\r', 116: '\t' };
class JObject { constructor() { this.names = []; this.values = []; } }
class JArray { constructor() { this.values = []; } }
class JString { constructor(v) { this.value = v; } }
class JNumber { constructor(t) { this.text = t; } }
class JLiteral { constructor(k) { this.kind = k; } }
class ParseError { constructor(message, offset) { this.message = message; this.offset = offset; } }
class Parser {
  constructor(text) { this.text = text; this.pos = 0; this.n = text.length; }
  current() { return this.pos < this.n ? this.text.charCodeAt(this.pos) : -1; }
  fail(message) { throw new ParseError(message, this.pos); }
  skipWs() { for (;;) { const c = this.current(); if (c !== 32 && c !== 9 && c !== 10 && c !== 13) break; this.pos++; } }
  readChar(c) { if (this.current() !== c) return false; this.pos++; return true; }
  expect(c, what) { if (!this.readChar(c)) this.fail("expected " + what); }
  isDigit(c) { return c >= 48 && c <= 57; }
  readDigits() { if (!this.isDigit(this.current())) this.fail("expected digit"); while (this.isDigit(this.current())) this.pos++; }
  readNumber() {
    const start = this.pos;
    this.readChar(45);
    const first = this.current();
    this.readDigits();
    if (first === 48 && this.pos - start > 1 + (this.text.charCodeAt(start) === 45 ? 1 : 0)) this.fail("leading zero");
    if (this.readChar(46)) this.readDigits();
    if (this.readChar(101) || this.readChar(69)) { if (!this.readChar(43)) this.readChar(45); this.readDigits(); }
    return new JNumber(this.text.slice(start, this.pos));
  }
  hexValue() {
    const c = this.current(); this.pos++;
    if (c >= 48 && c <= 57) return c - 48;
    if (c >= 97 && c <= 102) return c - 87;
    if (c >= 65 && c <= 70) return c - 55;
    this.fail("bad hex digit");
  }
  readString() {
    this.expect(34, "string");
    const parts = [];
    let start = this.pos;
    for (;;) {
      const c = this.current();
      if (c === 34) break;
      if (c < 0) this.fail("unterminated string");
      if (c === 92) {
        parts.push(this.text.slice(start, this.pos));
        this.pos++;
        const e = this.current(); this.pos++;
        if (e in ESC) parts.push(ESC[e]);
        else if (e === 117) {
          const cp = this.hexValue() * 4096 + this.hexValue() * 256 + this.hexValue() * 16 + this.hexValue();
          parts.push(String.fromCharCode(cp));
        } else this.fail("bad escape");
        start = this.pos;
      } else this.pos++;
    }
    parts.push(this.text.slice(start, this.pos));
    this.pos++;
    return parts.join("");
  }
  readLiteral(word, kind) { for (let i = 0; i < word.length; i++) this.expect(word.charCodeAt(i), word); return new JLiteral(kind); }
  readValue() {
    const c = this.current();
    if (c === 123) {
      this.pos++;
      const obj = new JObject();
      this.skipWs();
      if (this.readChar(125)) return obj;
      for (;;) {
        this.skipWs();
        obj.names.push(this.readString());
        this.skipWs(); this.expect(58, "':'"); this.skipWs();
        obj.values.push(this.readValue());
        this.skipWs();
        if (!this.readChar(44)) break;
      }
      this.expect(125, "'}'");
      return obj;
    }
    if (c === 91) {
      this.pos++;
      const arr = new JArray();
      this.skipWs();
      if (this.readChar(93)) return arr;
      for (;;) {
        this.skipWs();
        arr.values.push(this.readValue());
        this.skipWs();
        if (!this.readChar(44)) break;
      }
      this.expect(93, "']'");
      return arr;
    }
    if (c === 34) return new JString(this.readString());
    if (c === 116) return this.readLiteral("true", 1);
    if (c === 102) return this.readLiteral("false", 2);
    if (c === 110) return this.readLiteral("null", 0);
    if (c === 45 || this.isDigit(c)) return this.readNumber();
    this.fail("unexpected character");
  }
}
function parse(text) {
  const p = new Parser(text);
  p.skipWs();
  const v = p.readValue();
  p.skipWs();
  if (p.pos !== p.n) p.fail("trailing input");
  return v;
}
function walk(v) {
  if (v instanceof JObject) {
    let h = v.names.length * 7;
    for (const n of v.names) h += n.length;
    for (const x of v.values) h += walk(x);
    return h;
  }
  if (v instanceof JArray) { let h = 3; for (const x of v.values) h += walk(x); return h; }
  if (v instanceof JString) return 11 + v.value.length;
  if (v instanceof JNumber) return 13 + v.text.length;
  return 17 + v.kind;
}
function makeDoc(n) {
  let seed = 5;
  const records = [];
  for (let i = 0; i < n; i++) {
    seed = seed * 16807 % 2147483647;
    const tags = [];
    for (let k = 0; k < i % 5; k++) tags.push(`"t${seed % (k + 3)}"`);
    const sign = i % 3 === 0 ? "-" : "";
    const exp = i % 4 === 0 ? `e+${i % 5}` : i % 4 === 1 ? `E-${i % 3}` : "";
    const parent = i % 4 === 0 ? String(Math.floor(i / 4)) : "null";
    const active = i % 2 === 0 ? "true" : "false";
    records.push(`{"id": ${i}, "name": "user_${seed % 10007}", "score": ${sign}${seed % 1000}.${seed % 10}${exp}, "tags": [${tags.join(", ")}], `
      + `"active": ${active}, "parent": ${parent}, "bio": "line ${i}\\nwith \\"quotes\\", a tab\\tand caf\\u00e9 \\u2603 / slash \\\\ back", `
      + `"nested": {"depth": [{"x": [${i}, [${i + 1}, {"y": null}]]}]}}`);
  }
  return `{"records": [\n  ${records.join(",\n  ")}\n], "count": ${n}, "ok": true}`;
}

function main() {
  const doc = makeDoc(160);
  const bad = ["{\"a\": tru}", "[1, 2", "{\"a\" 1}", "[01]", "\"unterminated", "{\"a\": -}"];
  let total = 0;
  for (let i = 0; i < 15; i++) {
    total += walk(parse(doc));
    for (const b of bad) {
      let r;
      try { r = parse(b); } catch (e) { if (!(e instanceof ParseError)) throw e; r = new JLiteral(1000 + e.offset); }
      total += walk(r);
    }
  }
  return total;
}
