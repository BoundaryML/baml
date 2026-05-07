# BAML reference — deferred

When the BAML grader convention is finalized (see `tests/baml/README.md`
for the proposal), the canonical implementation goes here as
`text_stats.baml`. It would:

- Read each fixture via `baml.fs.open(path, "r").bytes()` /
  `.text()`.
- Compute byte / char / word / line counts.
- Compare against expected values baked into the program.
- Return `0` from `main()` if every fixture matches; non-zero
  otherwise.

A draft sketch is in `tests/baml/README.md` § "Reference implementation
(sketch)". Two language-level unknowns to resolve before it's
ship-ready:

1. Whether `String::length()` returns bytes or codepoints.
2. Whether `String::split_whitespace()` (or equivalent) exists.

Both can be answered by inspecting `baml_builtins2/src/` and BAML's
test corpus.
