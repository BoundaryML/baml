// An escape sequence that does not decode is reported once, where the
// literal is read. In an expression the message offers a raw string as the
// fix; in a type, a pattern or an attribute argument it does not, since a raw
// string would not parse there. Byte strings and backtick text have escape
// rules of their own, and a raw string has no escapes at all.

class Config {
  mode "fa\st" | "slow" @alias("mo\de")
  @@alias("con\gig")
}

enum Level {
  Low @alias("lo\w")
}

type Mode = "fa\st";

function literal_type(x: "a\d") -> int { 1 }

function patterns(x: string) -> int {
  match (x) {
    "fa\st" => 1,
    _ => 2,
  }
}

function expressions(x: string) -> string {
  let s = "bad\d";
  let b = b"\u{41}é\x80";
  let t = `one\q${x}two\z${for (let y in [x])}in\xZZ ${y}${endfor}${"\d"}`;
  let r = r"\d";
  let o = { "ke\y": 1 };
  s
}

function Prompt(x: string) -> string {
  client: "openai/gpt-4o"
  prompt: "hel\lo"
}

test "na\me" {
  let _unused = 1;
}
