function string_replace_literal_first() -> string { rep("a-b-", 2)._replace_literal("-", "+++") }
function string_replace_literal_missing() -> string { rep("a-b-", 2)._replace_literal("--", "+") }
function string_replace_literal_shorter() -> string { rep("abcabc", 1)._replace_literal("bc", "") }
function string_replace_literal_empty_search() -> string { rep("ab", 1)._replace_literal(rep("", 1), "+") }
function string_replace_literal_empty_string() -> string { `[${rep("", 1)._replace_literal("a", "+")}]` }
function string_replace_literal_both_empty() -> string { rep("", 1)._replace_literal(rep("", 1), "+") }
function string_replace_literal_wide() -> string {
  let r = rep("é😀é", 2)._replace_literal("😀", "éé");
  `${r}|${r.length()}|${r.byte_length()}`
}
function string_replace_literal_whole() -> string { rep("ab", 2)._replace_literal(rep("ab", 2), "é") }
function string_replace_first() -> string { rep("a-b-", 2).replace(rep("-", 1), rep("é", 2)) }
