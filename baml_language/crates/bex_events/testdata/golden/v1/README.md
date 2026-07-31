# Golden fixtures — v1 (FROZEN)

Byte-exact examples of every observability file class (TASK/design.md §6.9),
plus torn-tail variants truncated at every interesting offset. **`v1/` is
frozen forever**: once a fixture is committed here, its bytes never change.
A codec change that would alter any byte mints `v2/` alongside; readers must
keep parsing `v1/` for as long as the readers exist.

The verifier is `crates/bex_events/tests/golden_v1.rs`:

- Normal runs rebuild each fixture's bytes from the committed deterministic
  builder and assert **byte equality** with the file here — any encoder
  drift fails CI.
- Torn variants assert the *reader contract* (truncated flag vs explicit
  error) at each truncation offset, so recovery behavior is pinned too.
- `BAML_GOLDEN_WRITE=1 cargo test -p bex_events --test golden_v1` writes the
  files. Use it exactly once per new fixture; a diff on an existing file in
  review means an encoder changed and needs a `v2/`, not a regenerate.

| file | class | status |
|---|---|---|
| `events.bamlprof` | `.bamlprof` (header + event stream) | committed |
| `values.bamlvalue` | `.bamlvalue` (header + records) | committed |
| `*.bamlseg` / `*.bamlcct` | BCCT segments/snapshots | lands with P3 |
| `*.bamldict` | revision dictionary | lands with P1 |
| `*.bamlmeta` | BMET meta streams | lands with P3 |
| `*.bamlpack` / `*.bamlpack.idx` | value packs | lands with P5 |
| `*.bamlcids` | CID manifests | lands with P5 |
| canonical-value corpus + CIDs | value DAG canonical encoding | lands with P5 (C9) |
| `*.bamlidx` | exact-event index | lands with P6 |

Torn-tail variants are not stored as files: the verifier derives them by
truncating the committed bytes at the offsets listed in `golden_v1.rs`, so
the interesting-offset list is code-reviewed alongside the reader contract.
