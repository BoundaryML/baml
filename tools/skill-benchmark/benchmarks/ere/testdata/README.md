# Unified ERE corpus

`ere-corpus.json` contains 4,255 machine-readable assertions from:

- Rust regex's structured Fowler corpus
- All seven NetBSD AT&T `.dat` files
- All nineteen NetBSD regex `.in` files
- glibc `TESTS`, `PTESTS`, `rxspencer/tests`, `BOOST.tests`, and `PCRE.tests`
- ISC's regfuzz-generated validator corpus

Every assertion retains its raw source, file and line range, source semantics,
repository commit, file SHA-256, and license provenance. Cases outside
ERE-ASCII remain in the file with `ere_ascii.status` set to `excluded`.
Redundant assertions are marked `duplicate` and point to their canonical case.
Only `included` cases become native BAML tests.

The current normalization has 1,864 included cases, 609 duplicates, and 1,782
excluded cases.

The sixteen Fowler expectations changed to leftmost-first behavior by the Rust
conversion are restored to their original POSIX capture results in the
`ere_ascii` expectation. The untouched Rust expectation remains under
`source_expectation`.

Fourteen conflicting cross-corpus capture assertions are normalized to
ERE-ASCII's explicit numeric capture tie-break and final-participating-iteration
rules. Their original expectations remain under `source_expectation`.
