# programs::json parser 15

## eval-setup
```py
import json
seed = 5
records = []
for i in range(160):
    seed = seed * 16807 % 2147483647
    records.append({
        "id": i,
        "name": "user_%d" % (seed % 10007),
        "score": -(seed % 1000) / 8.0 if i % 3 == 0 else seed % 100000,
        "ratio": "%de-%d" % (seed % 97, i % 5),
        "tags": ["t%d" % (seed % (k + 3)) for k in range(i % 5)],
        "active": i % 2 == 0,
        "parent": None if i % 4 else i // 4,
        "bio": "line %d\nwith \"quotes\", a tab\tand café ☃ / slash \\ back" % i,
        "nested": {"depth": [{"x": [i, [i + 1, {"y": None}]]}]},
    })
doc = json.dumps({"records": records, "count": len(records), "ok": True}, ensure_ascii=True)
bad = ['{"a": tru}', '[1, 2', '{"a" 1}', '[01]', '"unterminated', '{"a": -}']
baml_doc = json.dumps(doc)
py_doc = repr(doc)
js_doc = json.dumps(doc)
baml_bad = json.dumps(bad)
py_bad = repr(bad)
js_bad = json.dumps(bad)
```

## BAML
```baml
// A hand-written recursive-descent JSON parser in the style of Are We Fast
// Yet's Json benchmark (a port of minimal-json, MIT): character scanning,
// string slicing and escape decoding, building a tree of union-typed values,
// and errors thrown and caught for malformed input. Parses a ~25 KB document
// 15 times plus six malformed inputs, then walks the trees.
class JObject { names: string[], values: JValue[] }
class JArray { values: JValue[] }
class JString { value: string }
class JNumber { text: string }
class JLiteral { kind: int }
type JValue = JObject | JArray | JString | JNumber | JLiteral
class ParseError { message: string, offset: int }
class Parser { text: string, cps: int[], pos: int }
function current(p: Parser) -> int {
  if (p.pos < p.cps.length()) { return p.cps[p.pos]; }
  return -1;
}
function fail(p: Parser, message: string) -> int {
  throw ParseError { message: message, offset: p.pos };
}
function skip_ws(p: Parser) -> int {
  while (true) {
    let c = current(p);
    if (c != 32 && c != 9 && c != 10 && c != 13) { break; }
    p.pos += 1;
  }
  0
}
function read_char(p: Parser, c: int) -> bool {
  if (current(p) != c) { return false; }
  p.pos += 1;
  true
}
function expect(p: Parser, c: int, what: string) -> int {
  if (!read_char(p, c)) { fail(p, "expected " + what); }
  0
}
function is_digit(c: int) -> bool { c >= 48 && c <= 57 }
function read_digits(p: Parser) -> int {
  if (!is_digit(current(p))) { fail(p, "expected digit"); }
  while (is_digit(current(p))) { p.pos += 1; }
  0
}
function read_number(p: Parser) -> JValue {
  let start = p.pos;
  read_char(p, 45);
  let first = current(p);
  read_digits(p);
  if (first == 48 && p.pos - start > 1 + (if (p.cps[start] == 45) { 1 } else { 0 })) {
    fail(p, "leading zero");
  }
  if (read_char(p, 46)) { read_digits(p); }
  if (read_char(p, 101) || read_char(p, 69)) {
    if (!read_char(p, 43)) { read_char(p, 45); }
    read_digits(p);
  }
  JNumber { text: p.text.slice(start, p.pos) }
}
function hex_value(p: Parser) -> int {
  let c = current(p);
  p.pos += 1;
  if (c >= 48 && c <= 57) { return c - 48; }
  if (c >= 97 && c <= 102) { return c - 87; }
  if (c >= 65 && c <= 70) { return c - 55; }
  fail(p, "bad hex digit")
}
function read_string(p: Parser) -> string {
  expect(p, 34, "string");
  let parts: string[] = [];
  let start = p.pos;
  while (true) {
    let c = current(p);
    if (c == 34) { break; }
    if (c < 0) { fail(p, "unterminated string"); }
    if (c == 92) {
      parts.push(p.text.slice(start, p.pos));
      p.pos += 1;
      let e = current(p);
      p.pos += 1;
      if (e == 34) { parts.push("\""); }
      else if (e == 92) { parts.push("\\"); }
      else if (e == 47) { parts.push("/"); }
      else if (e == 98) { parts.push(string.from_code_points([8])); }
      else if (e == 102) { parts.push(string.from_code_points([12])); }
      else if (e == 110) { parts.push("\n"); }
      else if (e == 114) { parts.push("\r"); }
      else if (e == 116) { parts.push("\t"); }
      else if (e == 117) {
        let cp = hex_value(p) * 4096 + hex_value(p) * 256 + hex_value(p) * 16 + hex_value(p);
        parts.push(string.from_code_points([cp]));
      } else { fail(p, "bad escape"); }
      start = p.pos;
    } else {
      p.pos += 1;
    }
  }
  parts.push(p.text.slice(start, p.pos));
  p.pos += 1;
  parts.join("")
}
function read_literal(p: Parser, word: string, kind: int) -> JValue {
  for (let c in word.to_code_points()) { expect(p, c, word); }
  JLiteral { kind: kind }
}
function read_value(p: Parser) -> JValue {
  let c = current(p);
  if (c == 123) {
    p.pos += 1;
    let obj = JObject { names: [], values: [] };
    skip_ws(p);
    if (read_char(p, 125)) { return obj; }
    while (true) {
      skip_ws(p);
      obj.names.push(read_string(p));
      skip_ws(p);
      expect(p, 58, "':'");
      skip_ws(p);
      obj.values.push(read_value(p));
      skip_ws(p);
      if (!read_char(p, 44)) { break; }
    }
    expect(p, 125, "'}'");
    return obj;
  }
  if (c == 91) {
    p.pos += 1;
    let arr = JArray { values: [] };
    skip_ws(p);
    if (read_char(p, 93)) { return arr; }
    while (true) {
      skip_ws(p);
      arr.values.push(read_value(p));
      skip_ws(p);
      if (!read_char(p, 44)) { break; }
    }
    expect(p, 93, "']'");
    return arr;
  }
  if (c == 34) { return JString { value: read_string(p) }; }
  if (c == 116) { return read_literal(p, "true", 1); }
  if (c == 102) { return read_literal(p, "false", 2); }
  if (c == 110) { return read_literal(p, "null", 0); }
  if (c == 45 || is_digit(c)) { return read_number(p); }
  fail(p, "unexpected character");
  JLiteral { kind: 0 }
}
function parse(text: string) -> JValue {
  let p = Parser { text: text, cps: text.to_code_points(), pos: 0 };
  skip_ws(p);
  let v = read_value(p);
  skip_ws(p);
  if (p.pos != p.cps.length()) { fail(p, "trailing input"); }
  v
}
function walk(v: JValue) -> int {
  match (v) {
    JObject => {
      let h = v.names.length() * 7;
      for (let n in v.names) { h += n.length(); }
      for (let x in v.values) { h += walk(x); }
      h
    },
    JArray => {
      let h = 3;
      for (let x in v.values) { h += walk(x); }
      h
    },
    JString => 11 + v.value.length(),
    JNumber => 13 + v.text.length(),
    JLiteral => 17 + v.kind,
  }
}
function main() -> int {
  let doc = $$baml_doc;
  let bad = $$baml_bad;
  let total = 0;
  for (let i = 0; i < 15; i += 1) {
    total += walk(parse(doc));
    for (let b in bad) {
      let r = parse(b) catch (e) { ParseError => JLiteral { kind: 1000 + e.offset } };
      total += walk(r);
    }
  }
  total
}
```

## Python
```python
class JObject:
    __slots__ = ('names', 'values')
    def __init__(self): self.names = []; self.values = []
class JArray:
    __slots__ = ('values',)
    def __init__(self): self.values = []
class JString:
    __slots__ = ('value',)
    def __init__(self, v): self.value = v
class JNumber:
    __slots__ = ('text',)
    def __init__(self, t): self.text = t
class JLiteral:
    __slots__ = ('kind',)
    def __init__(self, k): self.kind = k
class ParseError(Exception):
    def __init__(self, message, offset): self.message = message; self.offset = offset
ESC = {34: '"', 92: '\\', 47: '/', 98: '\b', 102: '\f', 110: '\n', 114: '\r', 116: '\t'}
class Parser:
    def __init__(self, text):
        self.text = text; self.cps = [ord(c) for c in text]; self.pos = 0
    def current(self):
        return self.cps[self.pos] if self.pos < len(self.cps) else -1
    def fail(self, message):
        raise ParseError(message, self.pos)
    def skip_ws(self):
        while self.current() in (32, 9, 10, 13): self.pos += 1
    def read_char(self, c):
        if self.current() != c: return False
        self.pos += 1
        return True
    def expect(self, c, what):
        if not self.read_char(c): self.fail("expected " + what)
    def read_digits(self):
        if not 48 <= self.current() <= 57: self.fail("expected digit")
        while 48 <= self.current() <= 57: self.pos += 1
    def read_number(self):
        start = self.pos
        self.read_char(45)
        first = self.current()
        self.read_digits()
        if first == 48 and self.pos - start > 1 + (1 if self.cps[start] == 45 else 0): self.fail("leading zero")
        if self.read_char(46): self.read_digits()
        if self.read_char(101) or self.read_char(69):
            if not self.read_char(43): self.read_char(45)
            self.read_digits()
        return JNumber(self.text[start:self.pos])
    def hex_value(self):
        c = self.current(); self.pos += 1
        if 48 <= c <= 57: return c - 48
        if 97 <= c <= 102: return c - 87
        if 65 <= c <= 70: return c - 55
        self.fail("bad hex digit")
    def read_string(self):
        self.expect(34, "string")
        parts = []
        start = self.pos
        while True:
            c = self.current()
            if c == 34: break
            if c < 0: self.fail("unterminated string")
            if c == 92:
                parts.append(self.text[start:self.pos])
                self.pos += 1
                e = self.current(); self.pos += 1
                if e in ESC: parts.append(ESC[e])
                elif e == 117:
                    cp = self.hex_value() * 4096 + self.hex_value() * 256 + self.hex_value() * 16 + self.hex_value()
                    parts.append(chr(cp))
                else: self.fail("bad escape")
                start = self.pos
            else:
                self.pos += 1
        parts.append(self.text[start:self.pos])
        self.pos += 1
        return "".join(parts)
    def read_literal(self, word, kind):
        for ch in word: self.expect(ord(ch), word)
        return JLiteral(kind)
    def read_value(self):
        c = self.current()
        if c == 123:
            self.pos += 1
            obj = JObject()
            self.skip_ws()
            if self.read_char(125): return obj
            while True:
                self.skip_ws()
                obj.names.append(self.read_string())
                self.skip_ws(); self.expect(58, "':'"); self.skip_ws()
                obj.values.append(self.read_value())
                self.skip_ws()
                if not self.read_char(44): break
            self.expect(125, "'}'")
            return obj
        if c == 91:
            self.pos += 1
            arr = JArray()
            self.skip_ws()
            if self.read_char(93): return arr
            while True:
                self.skip_ws()
                arr.values.append(self.read_value())
                self.skip_ws()
                if not self.read_char(44): break
            self.expect(93, "']'")
            return arr
        if c == 34: return JString(self.read_string())
        if c == 116: return self.read_literal("true", 1)
        if c == 102: return self.read_literal("false", 2)
        if c == 110: return self.read_literal("null", 0)
        if c == 45 or 48 <= c <= 57: return self.read_number()
        self.fail("unexpected character")
def parse(text):
    p = Parser(text)
    p.skip_ws()
    v = p.read_value()
    p.skip_ws()
    if p.pos != len(p.cps): p.fail("trailing input")
    return v
def walk(v):
    if isinstance(v, JObject):
        return len(v.names) * 7 + sum(len(n) for n in v.names) + sum(walk(x) for x in v.values)
    if isinstance(v, JArray):
        return 3 + sum(walk(x) for x in v.values)
    if isinstance(v, JString): return 11 + len(v.value)
    if isinstance(v, JNumber): return 13 + len(v.text)
    return 17 + v.kind
doc = $$py_doc
bad = $$py_bad
total = 0
for _ in range(15):
    total += walk(parse(doc))
    for b in bad:
        try:
            r = parse(b)
        except ParseError as e:
            r = JLiteral(1000 + e.offset)
        total += walk(r)
print(total)
```

## Typescript
```ts
class JObject { constructor() { this.names = []; this.values = []; } }
class JArray { constructor() { this.values = []; } }
class JString { constructor(v) { this.value = v; } }
class JNumber { constructor(t) { this.text = t; } }
class JLiteral { constructor(k) { this.kind = k; } }
class ParseError { constructor(message, offset) { this.message = message; this.offset = offset; } }
const ESC = { 34: '"', 92: '\\', 47: '/', 98: '\b', 102: '\f', 110: '\n', 114: '\r', 116: '\t' };
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
const doc = $$js_doc;
const bad = $$js_bad;
let total = 0;
for (let i = 0; i < 15; i++) {
  total += walk(parse(doc));
  for (const b of bad) {
    let r;
    try { r = parse(b); } catch (e) { if (!(e instanceof ParseError)) throw e; r = new JLiteral(1000 + e.offset); }
    total += walk(r);
  }
}
console.log(total);
```
