// Compiled JSON schemas against the interpreter: each probe decodes or
// encodes a type compiled code has a schema for, on the edges where the
// schema must agree with the interpreter or hand the call back to it.
class CodecPoint { x int y float on bool tag string note string? }
class CodecBox { points CodecPoint[] counts map<string, int> amount int | float level "low" | "high" }
class CodecSearch { tool "search" query string limit int? }
class CodecCalc { tool "calc" a int b int }
type CodecAction = CodecSearch | CodecCalc
class CodecNode { value int children CodecNode[] }
class CodecOmit { kept int dropped string? }

function decoded_or(text: string) -> string {
  let p = baml.json.decode<CodecPoint>(text) catch (e) {
    baml.json.DecodeError => { return "decode:" + e.message; },
    baml.json.ParseError => { return "parse:" + e.message; },
  };
  baml.json.stringify(p)
}

function action(text: string) -> string {
  let a = baml.json.decode<CodecAction>(text) catch (e) {
    baml.json.DecodeError => { return "decode:" + e.message; },
    baml.json.ParseError => { return "parse:" + e.message; },
  };
  match (a) {
    CodecSearch => "search:" + a.query + ":" + (a.limit ?? -1).to_string(),
    CodecCalc => "calc:" + (a.a + a.b).to_string(),
  }
}

function CodecScalars() -> string {
  let out: string[] = [];
  out.push(decoded_or("{\"x\": 1, \"y\": 2.5, \"on\": true, \"tag\": \"t\", \"note\": null}"));
  // A repeated key keeps its last value; an unknown key is skipped.
  out.push(decoded_or("{\"x\": 1, \"x\": 7, \"y\": 2, \"on\": false, \"tag\": \"é\\n\", \"extra\": [1, {\"a\": 2}]}"));
  // An integral float is an int; an absent optional is null.
  out.push(decoded_or("{\"x\": 3.0, \"y\": -0.0, \"on\": true, \"tag\": \"\"}"));
  // Each refusal is the interpreter's error.
  out.push(decoded_or("{\"x\": 1, \"y\": 2, \"on\": true}"));
  out.push(decoded_or("{\"x\": \"1\", \"y\": 2, \"on\": true, \"tag\": \"t\"}"));
  out.push(decoded_or("{\"x\": 1.5, \"y\": 2, \"on\": true, \"tag\": \"t\"}"));
  out.push(decoded_or("{\"x\": 9223372036854775808, \"y\": 2, \"on\": true, \"tag\": \"t\"}"));
  out.push(decoded_or("{\"x\": 1, \"y\": 2, \"on\": true, \"tag\": \"t\"} trailing"));
  out.push(decoded_or("{\"x\": 1, \"y\": 2"));
  out.push(decoded_or("[]"));
  out.join("|")
}

function CodecContainers() -> string {
  let text = "{\"points\": [{\"x\": 1, \"y\": 1e3, \"on\": true, \"tag\": \"a\"}], \"counts\": {\"b\": 2, \"a\": 1, \"b\": 3}, \"amount\": 12, \"level\": \"high\"}";
  let b = baml.json.decode<CodecBox>(text);
  let again = baml.json.stringify(b);
  let floaty = baml.json.decode<CodecBox>("{\"points\": [], \"counts\": {}, \"amount\": 12.5, \"level\": \"low\"}");
  let bad = baml.json.decode<CodecBox>("{\"points\": [], \"counts\": {}, \"amount\": 1, \"level\": \"mid\"}") catch (e) {
    baml.json.DecodeError => null,
  };
  let lists = baml.json.decode<int[][]>("[[1, 2], [], [3]]");
  again + "|" + baml.json.stringify(floaty) + "|" + (bad == null).to_string() + "|" + baml.json.stringify(lists)
}

function CodecUnions() -> string {
  let out: string[] = [];
  out.push(action("{\"tool\": \"search\", \"query\": \"q\", \"limit\": 3}"));
  out.push(action("{\"query\": \"q\", \"tool\": \"search\"}"));
  out.push(action("{\"tool\": \"calc\", \"a\": 2, \"b\": 5, \"query\": \"ignored\"}"));
  out.push(action("{\"tool\": \"browse\"}"));
  out.push(action("{\"tool\": \"calc\", \"a\": 2}"));
  out.push(action("{\"tool\": \"sea"));
  out.join("|")
}

function CodecRecursive() -> string {
  let n = baml.json.decode<CodecNode>("{\"value\": 1, \"children\": [{\"value\": 2, \"children\": []}, {\"value\": 3, \"children\": [{\"value\": 4, \"children\": []}]}]}");
  baml.json.stringify(n) + "|" + n.children[1].children[0].value.to_string()
}

function CodecEncode() -> string {
  let p = CodecPoint { x: -4, y: 0.1, on: false, tag: "q\"\\\t\u{1}", note: null };
  let o = CodecOmit { kept: 1, dropped: null };
  let o2 = CodecOmit { kept: 2, dropped: "x" };
  let nan = CodecPoint { x: 0, y: 0.0 / 0.0, on: true, tag: "", note: "n" };
  let refused = baml.json.stringify(nan) catch (e) {
    baml.json.SerializationError => "refused:" + e.message,
  };
  [
    baml.json.stringify(p),
    baml.json.stringify(p, omit_nulls = true),
    baml.json.stringify(o),
    baml.json.stringify(o2),
    baml.json.stringify(p, indent = 2),
    refused,
  ].join("|")
}
