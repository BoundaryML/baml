// Classes with a `$rust_type` field. A `DefaultHasher` holds its state as
// plain data in the heap, which the collector moves, and a `Regex` owns its
// compiled program, which the heap drops when the `Regex` is dead. The
// instances the natives return (`Regex`, `Match`, `Group`, `Error`) are
// built in Rust from the program's descriptions of the classes.

function state_hasher_new() -> int {
  baml.hash.DefaultHasher.new().finish()
}
function state_hasher_finish_twice() -> bool {
  let hasher = baml.hash.DefaultHasher.new();
  let first = hasher.finish();
  let kept = rep("k", 40);
  first == hasher.finish() && kept.length() == 40
}
function state_hashers_many() -> int {
  let hashers: baml.hash.DefaultHasher[] = [];
  for (let i = 0; i < 50; i += 1) { hashers.push(baml.hash.DefaultHasher.new()); };
  let same = 0;
  for (let h in hashers) { if h.finish() == hashers[0].finish() { same += 1; } };
  same
}

function state_regex_has_match() -> bool {
  baml.regex.new(rep("\\d", 1) + "+").has_match(rep("a", 3) + "1")
}
function state_regex_has_no_match() -> bool {
  baml.regex.new(rep("\\d", 1) + "+").has_match(rep("a", 3))
}
function state_regex_match_once() -> string {
  let found = baml.regex.new("(" + rep("\\d", 1) + "+)-(x)?").match_once(rep("ab ", 2) + "42- b");
  match (found) {
    null => "none",
    let m: baml.regex.Match => `${m.text} ${m.start} ${m.end} ${m.groups.length()} ${m.groups[1]?.text ?? "-"} ${m.groups[2] == null}`,
  }
}
function state_regex_match_none() -> bool {
  baml.regex.new(rep("\\d", 1) + "+").match_once(rep("abc", 2)) == null
}
// Offsets count characters, not bytes.
function state_regex_match_unicode() -> string {
  let found = baml.regex.new(rep("b", 1) + "+").match_once(rep("é", 3) + "bb" + rep("ü", 2));
  match (found) {
    null => "none",
    let m: baml.regex.Match => `${m.text} ${m.start} ${m.end}`,
  }
}
// Every name of the pattern is a key, whether or not its group took part.
function state_regex_named_groups() -> string {
  let pattern = "(?<year>" + rep("\\d", 1) + "{4})-(?<month>" + rep("\\d", 1) + "{2})(?<rest>z)?";
  let found = baml.regex.new(pattern).match_once(rep("on ", 1) + "2024-06 ok");
  match (found) {
    null => "none",
    let m: baml.regex.Match => `${m.named["year"]?.text ?? "-"} ${m.named["month"]?.start ?? 0 - 1} ${m.named["rest"] == null} ${m.named.length()}`,
  }
}
// A long match is a slice of the haystack, which stays alive with it.
function state_regex_long_match() -> int {
  let found = baml.regex.new("x" + rep("y", 1) + "+").match_once(rep("ab", 30) + "x" + rep("y", 200));
  match (found) {
    null => 0 - 1,
    let m: baml.regex.Match => m.text.length() + m.start,
  }
}
function state_regex_error_syntax() -> string {
  let made = baml.regex.new(rep("(", 2) + "a") catch (e) {
    baml.regex.Error => `${e.kind} ${e.pattern} ${e.span_start != null}`,
  };
  match (made) {
    string => made,
    _ => "compiled",
  }
}
function state_regex_error_unsupported() -> string {
  let made = baml.regex.new("(a)" + rep("\\1", 1)) catch (e) {
    baml.regex.Error => `${e.kind}`,
  };
  match (made) {
    string => made,
    _ => "compiled",
  }
}
function state_regex_error_uncaught() -> bool {
  baml.regex.new(rep("[", 1) + "a").has_match("a")
}
function state_regex_backtracking() -> bool {
  baml.regex.new("(a)" + rep("\\1", 1), backtracking = true).has_match(rep("a", 2))
}
// One program, matched many times while everything else moves.
function state_regex_reused() -> int {
  let digits = baml.regex.new(rep("\\d", 1) + "+");
  let found = 0;
  for (let i = 0; i < 40; i += 1) {
    let text = rep("a", i) + `${i}`;
    match (digits.match_once(text)) {
      null => {},
      let m: baml.regex.Match => { found += m.start; },
    }
  };
  found
}
// More programs than a collection is brought on by, each dead at once but
// the ones kept.
function state_regex_many() -> int {
  let kept: baml.regex.Regex[] = [];
  let matched = 0;
  for (let i = 0; i < 600; i += 1) {
    let made = baml.regex.new(`a{${i % 7 + 1}}`);
    if i % 100 == 0 { kept.push(made); }
    if made.has_match("aaaa") { matched += 1; }
  };
  for (let r in kept) { if r.has_match("a") { matched += 1000; } };
  matched
}
// A `Regex` held by a task that waits, and one in a union.
function state_regex_across_tasks() -> string {
  let digits = baml.regex.new(rep("\\d", 1) + "+");
  let a = spawn { let n = task_churn(40); digits.has_match(`${n}`) };
  let b = spawn { digits.match_once(rep("x", 3) + "7")?.start ?? 0 - 1 };
  `${await a} ${await b}`
}
