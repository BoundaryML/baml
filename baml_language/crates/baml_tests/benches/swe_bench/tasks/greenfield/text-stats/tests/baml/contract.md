# BAML cell contract

The proxy's BAML grader (`run_baml_grader` in `claude-proxy/src/main.rs`) is a
file-existence check: a candidate is considered to satisfy the cell iff
`text_stats.baml` exists at the staging-dir root after the agent finishes.

This contract is a v1 expedient because BAML lacks two language features
required to mirror the python/go graders: `argv` (no
`baml.sys.argv` / `baml.os.args`) and a generic stdout print
(`baml.io.input` is the only stdin/stdout I/O). See `README.md` in this
directory for the full proposal — exit-code-as-verdict — that becomes
viable once a `cross_lang_baml`-style runner is in place.

This file's purpose is twofold:

1. Pull the BAML cell into `enumerate_ready_cells()` enumeration in the
   worker (`benchmark-worker/src/main.rs:has_real_files`), which skips
   any `tests/<lang>/` directory that contains only a README. Without
   this file, BAML cells are silently filtered out and Suite B emits
   only python + go rows.
2. Document the contract close to where future implementers will look
   when they replace the file-existence check with a real driver.

Suite B's primary signals — turns, tool_calls, tokens, wall_clock_ms,
cost_usd, transcript — are captured by the proxy regardless of grader
verdict, so the BAML cell produces full agent-experience metrics today.
The grader's `passed` field is a coarse "did Claude produce a candidate
file?" signal until the full runner lands.
