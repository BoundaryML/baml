# BAML grader — deferred, with concrete blockers and a path forward

The BAML grader for the text-stats task **cannot be built as-is today**
with parity to the Python and Go graders. This file documents the
specific language-level gaps and a proposed convention that uses only
what BAML's stdlib already exposes.

## Why we can't mirror python/go directly

The text-stats spec says:

> Your program must be invokable as `<your-program> <path-to-input-file>`
> and print compact JSON on a single line to stdout.

This requires two BAML capabilities that **don't exist** in v1:

1. **CLI argument access** — no `baml.sys.argv`, `baml.os.args`, or
   equivalent. `baml.sys.shell`/`baml.sys.exec` are about RUNNING
   commands, not reading the program's own argv.
2. **Generic stdout print** — `baml.io.input(prompt)` is the only
   stdin/stdout I/O, and it goes the other way (reads a line from
   stdin, optionally with a prompt). There's no `baml.io.println` or
   `baml.print(s)`.

What BAML currently exposes:
- `baml.fs.{open,exists,read_dir,remove,size,write,write_bytes}`
- `baml.sys.{shell,exec}`
- `baml.io.input`
- Core string / array / class ops

## Proposed convention (works today)

Switch the grader contract for BAML cells to "exit code is the verdict",
which uses only existing BAML primitives:

- The candidate file is `text_stats.baml` at the staging-dir root.
- `main() -> int` reads each fixture from a known path under
  `inputs/`, computes the four stats (bytes/chars/words/lines),
  compares against the expected values that are baked into the
  candidate by the spec.
- Returns `0` if every fixture matches expected, non-zero if any
  fixture mismatches.

The proxy's BAML grader becomes:

```rust
async fn run_baml_grader(staging: &Path) -> Result<GraderResult> {
    let candidate = staging.join("text_stats.baml");
    if !candidate.is_file() { return Ok(GraderResult::default()); }
    let output = Command::new("cross_lang_baml")
        .arg(&candidate)
        .current_dir(staging)
        .output()
        .await?;
    let passed = output.status.success();   // exit 0 = pass
    Ok(GraderResult {
        passed,
        // For a single all-or-nothing verdict, we report 1 P2P "test"
        // and either 0 or 1 regressions. (Bugfix tasks would need a
        // richer convention.)
        pass_to_pass_total: Some(1),
        pass_to_pass_regressed: Some(if passed { 0 } else { 1 }),
        ..Default::default()
    })
}
```

Cost: one less granular than the Python/Go graders (they report per-test
pass/fail counts; this reports a single all-or-nothing). For a greenfield
task that's acceptable; for a bugfix task with FAIL_TO_PASS partitioning,
the candidate would need to encode the partition itself (return a bitmask?
return a count?), which is awkward.

## Reference implementation (sketch)

`reference/baml/text_stats.baml` would look like:

```baml
function check_one(path: string, want_bytes: int, want_chars: int,
                  want_words: int, want_lines: int) -> bool {
    let f1 = baml.fs.open(path, "r");
    let raw = f1.bytes();
    let f2 = baml.fs.open(path, "r");
    let text = f2.text();

    let bytes = raw.length();
    let chars = text.length();         // need to confirm: bytes vs codepoints?
    let words = text.split_whitespace().length();
    let lines = text.count("\n");

    bytes == want_bytes
      && chars == want_chars
      && words == want_words
      && lines == want_lines
}

function main() -> int {
    if !check_one("inputs/empty.txt",      0,  0, 0, 0)  { return 1; }
    if !check_one("inputs/hello.txt",     12, 12, 2, 1)  { return 2; }
    if !check_one("inputs/multi_line.txt", 34, 34, 6, 3) { return 3; }
    if !check_one("inputs/unicode.txt",   31, 24, 5, 2)  { return 4; }
    return 0;
}
```

Two unknowns to resolve before this is real:

- **Is `String::length()` bytes or codepoints?** If bytes, `chars` is wrong
  for unicode.txt (would equal 31, not 24). The `vm_string_concat_5k`
  benchmark uses `s.length()` after concatenating ASCII "hello"s, so
  it'd give the same value either way and isn't a discriminator.
- **Does BAML expose `split_whitespace`?** I haven't found a direct
  reference; may need `text.split(" ")` plus a manual filter for empty
  strings.

Both can be answered by inspecting `baml_builtins2/src/` and the
runtime test corpus more carefully.

## When this can ship

- **As-is**: implement the exit-code convention above. Greenfield BAML
  cells become full first-class. Bugfix BAML cells need a richer
  return-value convention.
- **Post language extension** (if `baml.sys.argv` + a stdout-print
  builtin land): the BAML grader can mirror python/go exactly, with
  per-test granularity matching the FAIL_TO_PASS / PASS_TO_PASS
  partitioning convention.

## Until then

The harness's `enumerate_ready_cells` filters out languages whose
`tests/<lang>/` directory has only README placeholders, so BAML cells
are skipped automatically. Suite B currently produces 2 cells per
greenfield task (python + go), not 3.
