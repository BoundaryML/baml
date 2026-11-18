function string_replace_all_literal_each() -> string { rep("a-b-", 2)._replace_all_literal("-", "+++") }
function string_replace_all_literal_missing() -> string { rep("a-b-", 2)._replace_all_literal("--", "+") }
function string_replace_all_literal_shorter() -> string { rep("abcabc", 1)._replace_all_literal("bc", "") }
function string_replace_all_literal_overlap() -> string { rep("a", 5)._replace_all_literal("aa", "b") }
function string_replace_all_literal_empty_search() -> string { rep("aé😀", 1)._replace_all_literal(rep("", 1), "+") }
function string_replace_all_literal_empty_string() -> string { `[${rep("", 1)._replace_all_literal("a", "+")}]` }
function string_replace_all_literal_both_empty() -> string { rep("", 1)._replace_all_literal(rep("", 1), "+") }
function string_replace_all_literal_wide() -> string {
  let r = rep("é😀é", 2)._replace_all_literal("é", "😀a");
  `${r}|${r.length()}|${r.byte_length()}`
}
function string_replace_all_literal_many() -> int { rep("ab ", 2000)._replace_all_literal(" ", "é😀").length() }
function string_replace_all_each() -> string { rep("a-b-", 2).replace_all(rep("-", 1), rep("é", 2)) }
