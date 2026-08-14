// Exhaustive match patterns — no diagnostics expected
enum Direction {
  North
  South
  East
  West
}

// All variants covered
function AllVariants(d: Direction) -> string {
  match (d) {
    Direction.North => "north",
    Direction.South => "south",
    Direction.East => "east",
    Direction.West => "west"
  }
}

// Wildcard catch-all
function WildcardCatchAll(d: Direction) -> string {
  match (d) {
    Direction.North => "north",
    _ => "other"
  }
}

// Named catch-all
function NamedCatchAll(d: Direction) -> string {
  match (d) {
    Direction.North => "north",
    let other => "other"
  }
}

// Boolean exhaustive
function BoolExhaustive(b: bool) -> string {
  match (b) {
    true => "yes",
    false => "no"
  }
}

// Optional with null
function OptionalExhaustive(x: int?) -> string {
  match (x) {
    let n: int => "has value",
    null => "null"
  }
}

// Type alias union fully covered
class Ok {
  value string
}
class Err {
  message string
}
type MyResult = Ok | Err

function TypeAliasExhaustive(r: MyResult) -> string {
  match (r) {
    let o: Ok => o.value,
    let e: Err => e.message
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// exhaustive.baml:1:1 (comment) len=56 "// Exhaustive match patterns — no diagnostics expected"
// exhaustive.baml:2:1 (keyword) len=4 "enum"
// exhaustive.baml:2:6 (enum) [declaration] len=9 "Direction"
// exhaustive.baml:3:3 (enumMember) [declaration] len=5 "North"
// exhaustive.baml:4:3 (enumMember) [declaration] len=5 "South"
// exhaustive.baml:5:3 (enumMember) [declaration] len=4 "East"
// exhaustive.baml:6:3 (enumMember) [declaration] len=4 "West"
// exhaustive.baml:9:1 (comment) len=23 "// All variants covered"
// exhaustive.baml:10:1 (keyword) len=8 "function"
// exhaustive.baml:10:10 (function) [declaration] len=11 "AllVariants"
// exhaustive.baml:10:22 (parameter) [declaration] len=1 "d"
// exhaustive.baml:10:25 (enum) len=9 "Direction"
// exhaustive.baml:10:39 (type) [defaultLibrary] len=6 "string"
// exhaustive.baml:11:3 (keyword) len=5 "match"
// exhaustive.baml:11:10 (parameter) len=1 "d"
// exhaustive.baml:12:5 (enum) len=9 "Direction"
// exhaustive.baml:12:14 (operator) len=1 "."
// exhaustive.baml:12:15 (enumMember) len=5 "North"
// exhaustive.baml:12:24 (string) len=7 "\"north\""
// exhaustive.baml:13:5 (enum) len=9 "Direction"
// exhaustive.baml:13:14 (operator) len=1 "."
// exhaustive.baml:13:15 (enumMember) len=5 "South"
// exhaustive.baml:13:24 (string) len=7 "\"south\""
// exhaustive.baml:14:5 (enum) len=9 "Direction"
// exhaustive.baml:14:14 (operator) len=1 "."
// exhaustive.baml:14:15 (enumMember) len=4 "East"
// exhaustive.baml:14:23 (string) len=6 "\"east\""
// exhaustive.baml:15:5 (enum) len=9 "Direction"
// exhaustive.baml:15:14 (operator) len=1 "."
// exhaustive.baml:15:15 (enumMember) len=4 "West"
// exhaustive.baml:15:23 (string) len=6 "\"west\""
// exhaustive.baml:19:1 (comment) len=21 "// Wildcard catch-all"
// exhaustive.baml:20:1 (keyword) len=8 "function"
// exhaustive.baml:20:10 (function) [declaration] len=16 "WildcardCatchAll"
// exhaustive.baml:20:27 (parameter) [declaration] len=1 "d"
// exhaustive.baml:20:30 (enum) len=9 "Direction"
// exhaustive.baml:20:44 (type) [defaultLibrary] len=6 "string"
// exhaustive.baml:21:3 (keyword) len=5 "match"
// exhaustive.baml:21:10 (parameter) len=1 "d"
// exhaustive.baml:22:5 (enum) len=9 "Direction"
// exhaustive.baml:22:14 (operator) len=1 "."
// exhaustive.baml:22:15 (enumMember) len=5 "North"
// exhaustive.baml:22:24 (string) len=7 "\"north\""
// exhaustive.baml:23:10 (string) len=7 "\"other\""
// exhaustive.baml:27:1 (comment) len=18 "// Named catch-all"
// exhaustive.baml:28:1 (keyword) len=8 "function"
// exhaustive.baml:28:10 (function) [declaration] len=13 "NamedCatchAll"
// exhaustive.baml:28:24 (parameter) [declaration] len=1 "d"
// exhaustive.baml:28:27 (enum) len=9 "Direction"
// exhaustive.baml:28:41 (type) [defaultLibrary] len=6 "string"
// exhaustive.baml:29:3 (keyword) len=5 "match"
// exhaustive.baml:29:10 (parameter) len=1 "d"
// exhaustive.baml:30:5 (enum) len=9 "Direction"
// exhaustive.baml:30:14 (operator) len=1 "."
// exhaustive.baml:30:15 (enumMember) len=5 "North"
// exhaustive.baml:30:24 (string) len=7 "\"north\""
// exhaustive.baml:31:5 (keyword) len=3 "let"
// exhaustive.baml:31:9 (variable) [declaration] len=5 "other"
// exhaustive.baml:31:18 (string) len=7 "\"other\""
// exhaustive.baml:35:1 (comment) len=21 "// Boolean exhaustive"
// exhaustive.baml:36:1 (keyword) len=8 "function"
// exhaustive.baml:36:10 (function) [declaration] len=14 "BoolExhaustive"
// exhaustive.baml:36:25 (parameter) [declaration] len=1 "b"
// exhaustive.baml:36:28 (type) [defaultLibrary] len=4 "bool"
// exhaustive.baml:36:37 (type) [defaultLibrary] len=6 "string"
// exhaustive.baml:37:3 (keyword) len=5 "match"
// exhaustive.baml:37:10 (parameter) len=1 "b"
// exhaustive.baml:38:5 (type) len=4 "true"
// exhaustive.baml:38:13 (string) len=5 "\"yes\""
// exhaustive.baml:39:5 (type) len=5 "false"
// exhaustive.baml:39:14 (string) len=4 "\"no\""
// exhaustive.baml:43:1 (comment) len=21 "// Optional with null"
// exhaustive.baml:44:1 (keyword) len=8 "function"
// exhaustive.baml:44:10 (function) [declaration] len=18 "OptionalExhaustive"
// exhaustive.baml:44:29 (parameter) [declaration] len=1 "x"
// exhaustive.baml:44:32 (type) [defaultLibrary] len=3 "int"
// exhaustive.baml:44:41 (type) [defaultLibrary] len=6 "string"
// exhaustive.baml:45:3 (keyword) len=5 "match"
// exhaustive.baml:45:10 (parameter) len=1 "x"
// exhaustive.baml:46:5 (keyword) len=3 "let"
// exhaustive.baml:46:9 (variable) [declaration] len=1 "n"
// exhaustive.baml:46:12 (type) [defaultLibrary] len=3 "int"
// exhaustive.baml:46:19 (string) len=11 "\"has value\""
// exhaustive.baml:47:5 (type) [defaultLibrary] len=4 "null"
// exhaustive.baml:47:13 (string) len=6 "\"null\""
// exhaustive.baml:51:1 (comment) len=33 "// Type alias union fully covered"
// exhaustive.baml:52:1 (keyword) len=5 "class"
// exhaustive.baml:52:7 (class) [declaration] len=2 "Ok"
// exhaustive.baml:53:3 (property) [declaration] len=5 "value"
// exhaustive.baml:53:9 (type) [defaultLibrary] len=6 "string"
// exhaustive.baml:55:1 (keyword) len=5 "class"
// exhaustive.baml:55:7 (class) [declaration] len=3 "Err"
// exhaustive.baml:56:3 (property) [declaration] len=7 "message"
// exhaustive.baml:56:11 (type) [defaultLibrary] len=6 "string"
// exhaustive.baml:58:1 (keyword) len=4 "type"
// exhaustive.baml:58:6 (type) [declaration] len=8 "MyResult"
// exhaustive.baml:58:15 (operator) len=1 "="
// exhaustive.baml:58:17 (class) len=2 "Ok"
// exhaustive.baml:58:20 (operator) len=1 "|"
// exhaustive.baml:58:22 (class) len=3 "Err"
// exhaustive.baml:60:1 (keyword) len=8 "function"
// exhaustive.baml:60:10 (function) [declaration] len=19 "TypeAliasExhaustive"
// exhaustive.baml:60:30 (parameter) [declaration] len=1 "r"
// exhaustive.baml:60:33 (type) len=8 "MyResult"
// exhaustive.baml:60:46 (type) [defaultLibrary] len=6 "string"
// exhaustive.baml:61:3 (keyword) len=5 "match"
// exhaustive.baml:61:10 (parameter) len=1 "r"
// exhaustive.baml:62:5 (keyword) len=3 "let"
// exhaustive.baml:62:9 (variable) [declaration] len=1 "o"
// exhaustive.baml:62:12 (class) len=2 "Ok"
// exhaustive.baml:62:18 (variable) len=1 "o"
// exhaustive.baml:62:20 (property) len=5 "value"
// exhaustive.baml:63:5 (keyword) len=3 "let"
// exhaustive.baml:63:9 (variable) [declaration] len=1 "e"
// exhaustive.baml:63:12 (class) len=3 "Err"
// exhaustive.baml:63:19 (variable) len=1 "e"
// exhaustive.baml:63:21 (property) len=7 "message"
