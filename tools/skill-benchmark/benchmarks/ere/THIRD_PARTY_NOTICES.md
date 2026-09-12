# Third-party notices

`testdata/ere-corpus.json` retains the source repository, pinned commit, source
path, source line range, SHA-256 hash, raw assertion, and license identifier for
every normalized case.

## Rust regex Fowler corpus

- Repository: https://github.com/rust-lang/regex
- Pinned commit: `72d650cb0a880a01ab6dc2137c0888e8f89740f7`
- Source paths: `testdata/fowler/basic.toml`, `nullsubexpr.toml`, and `repetition.toml`
- License: MIT OR Apache-2.0

## NetBSD regex corpora

- Repository: https://github.com/NetBSD/src
- Pinned commit: `5a7765109d6feb3a897b349b8d58572a0b7812f2`
- Source paths: `tests/lib/libc/regex/data/att/*.dat` and the `.in` files listed in the corpus metadata
- AT&T data license: license-free test data as stated by `data/att/README`
- Other data: NetBSD source licenses recorded in the source file headers

## glibc regex corpora

- Repository: https://sourceware.org/git/glibc.git
- Pinned commit: `04e750e75b73957cf1c791535a3f4319534a52fc`
- `posix/TESTS` and `posix/PTESTS`: LGPL-2.1-or-later
- `posix/BOOST.tests`: Boost Software License 1.0
- `posix/PCRE.tests`: PCRE license embedded in the source corpus
- `posix/rxspencer/tests`: BSD-style license in `posix/rxspencer/COPYRIGHT`

## ISC regfuzz corpus

- Repository: https://github.com/Stichting-MINIX-Research-Foundation/minix
- Pinned commit: `4db99f4012570a577414fe2a43697b2f239b699e`
- Source path: `external/bsd/bind/dist/lib/isc/tests/regex_test.c`
- License: ISC

The benchmark-specific interface, profile, tests, and corpus normalization are
not derived from these projects.
