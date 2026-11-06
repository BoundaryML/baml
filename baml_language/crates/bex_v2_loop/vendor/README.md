# Private corosensei fork

Source: corosensei 0.3.4 crates.io source archive (upstream VCS revision
`4b5db4845204a4b2663d3f7f75866be4cc3c680d`), copied from the runtime-v2 GC
experiment's `research-sources/corosensei-0.3.4`. MIT OR Apache-2.0; both licenses
are retained. Cargo.toml.orig retains the upstream authored manifest; `.cargo_vcs_info.json` identifies upstream.

Runtime change: `Coroutine::saved_sp() -> Option<usize>` exposes the existing
private `stack_ptr`. No layout guessing, assembly changes, or altered context
switches. The experiment's accessor has expanded safety documentation here.
Upstream 0.3.4 has no public accessor, so the fork is necessary for precise scan
bounds. `Stack::limit()` includes the inaccessible guard page and is not a valid
scan start. Suspended `saved_sp()` includes the saved ABI registers.

Packaging changes: remove upstream dev dependencies, example/benchmark targets,
and profiles from the normalized Cargo.toml, and omit its generated Cargo.lock.
Upstream source/tests/examples remain for reference; this trimmed package is a
runtime dependency, not a standalone checkout for running upstream's test suite.
The workspace excludes this directory so Cargo does not adopt it as a workspace
member or apply BAML's internal-crate lint policies to upstream build code.
No global `[patch]`: existing crates continue using registry corosensei unchanged.

A future upstream release exposing the same contract can replace this fork after
re-running debug/release root coverage, cancellation/unwinding, and memory tests.
