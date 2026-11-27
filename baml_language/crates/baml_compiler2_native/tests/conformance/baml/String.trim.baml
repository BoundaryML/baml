function string_trim_both_sides() -> string { `[${`${rep(" ", 3)}a b${rep(" ", 2)}`.trim()}]` }
function string_trim_nothing() -> string { `[${rep("ab ", 2).slice(0, 5).trim()}]` }
function string_trim_all_whitespace() -> string { `[${rep(" ", 4).trim()}]` }
function string_trim_empty() -> string { `[${rep(" ", 0).trim()}]` }
function string_trim_every_ascii_space() -> string { `[${rep(" \t\n\r", 2).trim()}|${`${rep("\t\n", 2)}a\tb${rep("\r ", 2)}`.trim()}]` }
function string_trim_past_ascii() -> string {
  let wide = rep("　", 2);
  let s = ` ${wide} é ü${wide} `;
  `[${s.trim()}|${s.trim_start().length()}|${s.trim_end().length()}|${`${wide}`.trim().length()}]`
}
function string_trim_keeps_what_is_not_space() -> string {
  let s = `${rep("é", 1)} x ${rep("​", 1)}`;
  `${s.trim().length()}|${s.trim_start().length()}|${s.trim_end().length()}`
}
