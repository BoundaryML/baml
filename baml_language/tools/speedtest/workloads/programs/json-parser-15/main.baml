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
// The input: 160 records exercising every JSON form, with escapes in strings
// and numbers in every shape, built from a seeded LCG.
function make_doc(n: int) -> string {
  let seed = 5;
  let records: string[] = [];
  for (let i = 0; i < n; i += 1) {
    seed = seed * 16807 % 2147483647;
    let tags: string[] = [];
    for (let k = 0; k < i % 5; k += 1) { tags.push("\"t" + (seed % (k + 3)).to_string() + "\""); }
    let sign = if (i % 3 == 0) { "-" } else { "" };
    let exp = if (i % 4 == 0) { "e+" + (i % 5).to_string() } else if (i % 4 == 1) { "E-" + (i % 3).to_string() } else { "" };
    let parent = if (i % 4 == 0) { (i / 4).to_string() } else { "null" };
    let active = if (i % 2 == 0) { "true" } else { "false" };
    records.push("{\"id\": " + i.to_string() + ", \"name\": \"user_" + (seed % 10007).to_string() + "\", \"score\": " + sign
      + (seed % 1000).to_string() + "." + (seed % 10).to_string() + exp + ", \"tags\": [" + tags.join(", ") + "], \"active\": " + active
      + ", \"parent\": " + parent + ", \"bio\": \"line " + i.to_string() + "\\nwith \\\"quotes\\\", a tab\\tand caf\\u00e9 \\u2603 / slash \\\\ back\""
      + ", \"nested\": {\"depth\": [{\"x\": [" + i.to_string() + ", [" + (i + 1).to_string() + ", {\"y\": null}]]}]}}");
  }
  "{\"records\": [\n  " + records.join(",\n  ") + "\n], \"count\": " + n.to_string() + ", \"ok\": true}"
}

function main() -> int {
  let doc = make_doc(160);
  let bad = ["{\"a\": tru}", "[1, 2", "{\"a\" 1}", "[01]", "\"unterminated", "{\"a\": -}"];
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
