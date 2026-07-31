# paths/ — generated distinct-context workloads

C4's bytes/path gate runs over programs with `P` distinct calling contexts at a
constant total call count, `P ∈ {4 .. 4096}` (bracketing the corpus p99 = observed
max of 3,537 CCT nodes). Files here are emitted by the deterministic generator:

```
obs-bench gen-paths --contexts 4 --contexts 64 --contexts 512 --contexts 4096 \
    --total-calls 1000000 --out crates/tools_obs_bench/workloads/paths/
```

Generated files are `gen-paths-P<contexts>.baml` and are NOT committed (only this
README is): the generator is seeded and byte-deterministic, so CI regenerates them
on demand and any drift is a generator bug, not a fixture edit.

Shape per file: a binary call tree of distinct helper functions wide enough to
yield exactly `P` distinct (parent, function) contexts, driven by a loop that
holds total profiled calls at `--total-calls` regardless of `P`.
