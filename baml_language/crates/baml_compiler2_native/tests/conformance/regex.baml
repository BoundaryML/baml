// `baml.regex` past compiling and the first match: every match, a whole
// match, splitting, templates, and the literal helpers.

function regex_digits(n: int) -> baml.regex.Regex {
  baml.regex.new("(" + rep("\\d", n) + "+)(x)?")
}
function regex_shown(found: baml.regex.Match[]) -> string {
  let out = "";
  for (let m in found) {
    out = `${out}[${m.text} ${m.start}..${m.end} ${m.groups.length()} ${m.groups[2] == null}]`;
  };
  out
}

function regex_match_all() -> string {
  regex_shown(regex_digits(1).match_all(rep("a1 22x ", 2) + "333"))
}
function regex_match_all_none() -> int {
  regex_digits(1).match_all(rep("abc", 4)).length()
}
// Offsets count characters, and are found in one pass over the text.
function regex_match_all_unicode() -> string {
  regex_shown(regex_digits(1).match_all(rep("é7ü", 3) + "42x"))
}
function regex_match_all_empty_matches() -> string {
  regex_shown(baml.regex.new(rep("z", 1) + "*").match_all(rep("éz", 2)))
}
function regex_match_all_named() -> string {
  let found = baml.regex.new("(?<k>[a-z]+)=(?<v>" + rep("\\d", 1) + "+)?").match_all(rep("a=1 b= ", 2));
  let out = "";
  for (let m in found) {
    out = `${out}${m.named["k"]?.text ?? "-"}:${m.named["v"]?.text ?? "-"} `;
  };
  out
}
function regex_match_all_many() -> int {
  let found = regex_digits(1).match_all(rep("12 ", 500));
  found.length() + found[499].start
}
function regex_match_all_long() -> int {
  let found = baml.regex.new("x" + rep("y", 1) + "+").match_all(rep("x" + rep("y", 100) + " ", 3));
  found[2].text.length() + found[2].start
}

function regex_match_full() -> string {
  match (regex_digits(1).match_full(rep("12", 2) + "x")) {
    null => "none",
    let m: baml.regex.Match => `${m.text} ${m.start} ${m.end} ${m.groups[1]?.text ?? "-"}`,
  }
}
function regex_match_full_partial() -> bool {
  regex_digits(1).match_full(rep("12", 2) + "x ") == null
}
function regex_match_full_empty() -> bool {
  baml.regex.new(rep("a", 1) + "*").match_full(rep("a", 0)) == null
}

function regex_split() -> string {
  baml.regex.new(rep(",", 1) + "\\s*").split(rep("a, b,c,  ", 2) + "d").join("|")
}
function regex_split_nothing() -> string {
  baml.regex.new(rep(",", 1)).split(rep("abc", 2)).join("|")
}
function regex_split_empty() -> int {
  baml.regex.new(rep(",", 1)).split(rep("abc", 0)).length()
}
function regex_split_at_the_ends() -> string {
  baml.regex.new(rep(",", 1)).split("," + rep("a,", 2)).join("|")
}
// What a group of the delimiter took is a field too.
function regex_split_with_groups() -> string {
  baml.regex.new("(" + rep("-", 1) + ")|(\\+)").split(rep("a-b+c", 2)).join("|")
}
function regex_split_unicode() -> string {
  baml.regex.new(rep("ü", 1) + "+").split(rep("éüüé", 2)).join("|")
}
function regex_split_long_fields() -> int {
  let fields = baml.regex.new(rep(";", 1)).split(rep(rep("ab", 40) + ";", 3));
  fields.length() + fields[1].length()
}

function regex_replace() -> string {
  rep("a1b22", 2).replace(regex_digits(1), "<$1>")
}
function regex_replace_all() -> string {
  rep("a1b22x", 2).replace_all(regex_digits(1), "<$1$2>")
}
function regex_replace_named() -> string {
  rep("k=v ", 2).replace_all(baml.regex.new("(?<key>[a-z])=(?<value>[a-z])"), rep("$", 1) + "{value}=$" + "{key}")
}
// Nothing matched: the haystack itself.
function regex_replace_none() -> string {
  rep("abc", 3).replace_all(regex_digits(1), "#")
}
function regex_replace_unicode() -> string {
  rep("é1ü", 2).replace_all(regex_digits(1), "→")
}
function regex_replace_with_closure() -> string {
  baml.regex._replace_pattern_with(rep("a1b22", 2), regex_digits(1), (m: baml.regex.Match) -> string { `${m.start}` }, true)
}
function regex_replace_literal_with_closure() -> string {
  baml.regex._replace_literal_with(rep("aéa", 2), "a", (m: baml.regex.Match) -> string { `${m.start}` }, true)
}
function regex_replace_empty_literal_with_closure() -> string {
  baml.regex._replace_literal_with(rep("éa", 2), "", (m: baml.regex.Match) -> string { `${m.start}` }, true)
}
function regex_replace_first_with_closure() -> string {
  baml.regex._replace_literal_with(rep("xay", 2), "a", (m: baml.regex.Match) -> string { m.text + m.text }, false)
}

function regex_escape() -> string {
  baml.regex.escape(rep("a.b*c", 2) + "(d)[e]{f}^$|\\?+")
}
function regex_escape_nothing() -> string {
  baml.regex.escape(rep("abc é", 2))
}
function regex_escaped_matches_itself() -> bool {
  let literal = rep("1+1=2?", 2);
  baml.regex.new(baml.regex.escape(literal)).match_full(literal) != null
}

function regex_backtracking_match_all() -> int {
  baml.regex.new("(a)" + rep("\\1", 1), backtracking = true).match_all(rep("aa b ", 3)).length()
}
