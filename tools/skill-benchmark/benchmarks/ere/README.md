# ERE benchmark

Implement an ASCII POSIX Extended Regular Expression search engine entirely in
BAML.

The only callable interface under evaluation is:

```baml
function exec_ere(
    pattern: string,
    input: string,
) -> EreMatch? throws baml.errors.ParseError
```

Read [SPEC.md](SPEC.md) before implementing it. The specification defines the
exact supported profile and takes precedence over other regex implementations.

## Run

```bash
baml check
baml test --list
baml test
```

The starter implementation deliberately throws `ParseError("not implemented")`.
The valid-match tests therefore fail until `exec_ere` is implemented. The
invalid-pattern tests pass against the stub and must continue passing for the
right reason.

You may replace `baml_src/ere.baml` and add supporting `.baml` files. Do not
change the public classes, the `exec_ere` signature, the specification, or the
public tests.

## Evaluation

The public suite dynamically registers every compatible, nonduplicate assertion
from `testdata/ere-corpus.json` as a native BAML test. The JSON retains all
source assertions, including incompatible cases and duplicates, with their
classification and provenance.

Reference material:

- POSIX.1-2024, Base Definitions, Chapter 9: https://pubs.opengroup.org/onlinepubs/9799919799/
- Fowler corpus as preserved by Rust regex: https://github.com/rust-lang/regex/tree/master/testdata/fowler
- NetBSD regex tests: https://github.com/NetBSD/src/tree/trunk/tests/lib/libc/regex
- glibc regex corpora: https://sourceware.org/git/?p=glibc.git;a=tree;f=posix
- ISC regfuzz corpus: https://github.com/Stichting-MINIX-Research-Foundation/minix/blob/master/external/bsd/bind/dist/lib/isc/tests/regex_test.c

See `THIRD_PARTY_NOTICES.md` for exact pinned revisions and licensing.
