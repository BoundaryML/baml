function string_lines_plain() -> string {
  let l = rep("one\ntwo\r\nthree\n", 2).lines();
  `${l.length()}:${l[0]}|${l[1]}|${l[2]}|${l[5]}|${l[1].length()}`
}
function string_lines_no_ending() -> string {
  let l = rep("é😀\n\nlast", 1).lines();
  `${l.length()}:${l[0]}|${l[1]}|${l[2]}|${l[0].length()}`
}
function string_lines_lone_return() -> string {
  let l = rep("a\rb\n", 1).lines();
  `${l.length()}:${l[0].length()}`
}
function string_lines_empty() -> int { rep("a", 0).lines().length() }
function string_lines_only_endings() -> int { rep("\n", 3).lines().length() }
