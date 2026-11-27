// `baml.path`: paths as the host writes them.

function path_join() -> string { baml.path.join([rep("a", 2), "b", rep("c.txt", 1)]) }
function path_join_nothing() -> string { baml.path.join([]) }
function path_join_one() -> string { baml.path.join([rep("only", 1)]) }
// A part that is absolute replaces what came before it.
function path_join_absolute_part() -> string { baml.path.join([rep("a", 1), "/b", rep("c", 1)]) }
function path_join_empty_parts() -> string { baml.path.join([rep("a", 1), "", rep("b", 1), ""]) }
function path_join_keeps_dots() -> string { baml.path.join([rep("a", 1), "..", ".", rep("b/", 1)]) }
function path_join_past_ascii() -> string { baml.path.join([rep("é", 2), rep("→", 1)]) }

function path_parent() -> string { baml.path.parent(rep("/a/b/", 1) + "c.txt") ?? "none" }
function path_parent_of_root() -> string { baml.path.parent(rep("/", 1)) ?? "none" }
function path_parent_of_a_name() -> string { `[${baml.path.parent(rep("name", 1)) ?? "none"}]` }
function path_parent_of_nothing() -> string { baml.path.parent(rep("x", 0)) ?? "none" }
function path_parent_trailing_slash() -> string { baml.path.parent(rep("a/b/", 1)) ?? "none" }
function path_parent_long() -> int { (baml.path.parent(rep("dir/", 40) + "file") ?? "").length() }

function path_is_absolute() -> string {
  `${baml.path.is_absolute(rep("/a", 1))} ${baml.path.is_absolute(rep("a/b", 1))} ${baml.path.is_absolute(rep("", 1))} ${baml.path.is_absolute(rep("~/a", 1))}`
}

function path_absolute_under_a_base() -> string { baml.path.absolute(rep("b/c", 1), base = rep("/a", 1)) }
function path_absolute_already() -> string { baml.path.absolute(rep("/x/y", 1), base = rep("/a", 1)) }
function path_absolute_keeps_dot_dot() -> string { baml.path.absolute(rep("../c", 1), base = rep("/a/b", 1)) }
function path_absolute_drops_dot() -> string { baml.path.absolute(rep("./c/.", 1), base = rep("/a", 1)) }
// Under the directory the program runs in, which is the test's own.
function path_absolute_under_the_working_directory() -> bool {
  let found = baml.path.absolute(rep("some/file", 1));
  baml.path.is_absolute(found) && found.ends_with("/some/file")
}
function path_absolute_home() -> bool {
  let found = baml.path.absolute(rep("~/notes", 1));
  baml.path.is_absolute(found) && found.ends_with("/notes") && !found.includes("~")
}
