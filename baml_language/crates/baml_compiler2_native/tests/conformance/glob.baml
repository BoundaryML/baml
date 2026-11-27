// `baml.glob`: compiling a pattern and matching a path against it.

function glob_matches() -> string {
  let glob = baml.glob.new(rep("src/**/*", 1) + ".rs");
  `${glob.matches(rep("src/a/b/main", 1) + ".rs")} ${glob.matches("src/main.rs")} ${glob.matches("src/main.ts")} ${glob.matches("lib/main.rs")}`
}
function glob_star_stays_in_a_directory() -> string {
  let glob = baml.glob.new(rep("*", 1) + ".txt");
  `${glob.matches(rep("a", 3) + ".txt")} ${glob.matches("dir/a.txt")} ${glob.matches(".txt")}`
}
function glob_question_and_classes() -> string {
  let glob = baml.glob.new(rep("file", 1) + "?.[ch]");
  `${glob.matches("file1.c")} ${glob.matches("file1.h")} ${glob.matches("file12.c")} ${glob.matches("file1.o")}`
}
function glob_braces() -> string {
  let glob = baml.glob.new(rep("*.", 1) + "{png,jpg}");
  `${glob.matches("a.png")} ${glob.matches("a.jpg")} ${glob.matches("a.gif")}`
}
function glob_literal() -> string {
  let glob = baml.glob.new(rep("exact/path", 1));
  `${glob.matches("exact/path")} ${glob.matches("exact/path/more")} ${glob.matches("")}`
}
function glob_past_ascii() -> bool {
  baml.glob.new(rep("é", 1) + "*/→?").matches(rep("é", 3) + "/→ü")
}
function glob_bad_pattern() -> bool {
  baml.glob.new(rep("[", 1) + "a").matches("a")
}
function glob_bad_pattern_caught() -> string {
  let glob = baml.glob.new(rep("{", 1) + "a,b") catch (e) {
    baml.errors.ParseError => { return e.message; },
  };
  `${glob.matches("a")}`
}
function glob_reused() -> int {
  let glob = baml.glob.new(rep("**/*", 1) + ".md");
  let found = 0;
  for (let i = 0; i < 200; i += 1) {
    if (glob.matches(rep("d/", i % 5) + "readme" + (if (i % 3 == 0) { ".md" } else { ".txt" }))) { found += 1; };
  };
  found
}
function glob_many() -> int {
  let found = 0;
  for (let i = 0; i < 300; i += 1) {
    if (baml.glob.new(rep("a", i % 7) + "*").matches(rep("a", 3) + "b")) { found += 1; };
  };
  found
}
