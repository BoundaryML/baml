# Observability workloads

These plain BAML programs are stable workload shapes for the observability
benchmark harness. They deliberately contain no provider or network calls.

- `hotloop`: fixed call-count and fixed-rate CPU loops.
- `agent`: 86-way spawned work, depth-14 stacks, awaits, sysops, and caught errors.
- `transcript`: 64 KiB/1 MiB append-only transcript prefixes for 16–128 captures.
- `idle`: a live boundary with no calling-context churn.
- `deep`: recursion depths 200 and 1024.
- `paths`: the checked-in input matrix for `obs-bench gen-paths`.

Large inputs are generated at run time. `obs-bench corpus synth` defaults to a
small deterministic corpus, while `--target-bytes 10737418240` creates the full
10 GiB-per-mode design workload as deterministic hard-linked shards. Each mode
has one physical template encoded below 9 MiB, and the tool caps the manifest
below 10,000 supported artifact paths.
