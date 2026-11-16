# Test cases

Each `.case` file is one run of stow against a small workspace. The format is plain text so
that any implementation can be tested with the same files.

## Format

A case is a list of sections. A section starts with a header line beginning `--- ` and
runs to the next header.

| Header | Body |
|---|---|
| `--- args` | Command-line arguments, one per line. |
| `--- cwd` | Directory to run in, relative to the workspace. Default: the workspace root. |
| `--- file <path>` | A file to create before the run. |
| `--- exit` | The expected exit code. Required. |
| `--- stdout` | The expected standard output, compared exactly. |
| `--- expect <path>` | The expected content of a file the run creates or changes. |

Standard error is not compared.

## Rules for a runner

1. Create an empty temporary directory and write every `file` section into it.
2. For every `Cargo.toml` that contains a `[package]` line, also create an empty
   `src/lib.rs` beside it unless the case provides one. Cargo rejects a package with
   no target.
3. Run stow with the `args` in the `cwd`.
4. Compare the exit code, and standard output if the case has a `stdout` section.
5. Every file that was created or whose content changed must have a matching `expect`
   section, and every `expect` section must match such a file. So an input file with no
   `expect` section must be byte-identical after the run.

## Running

`cargo test -p cargo-stow` runs every case against this crate's binary.

- `STOW_BIN=/path/to/executable` tests another implementation. It is invoked with the
  case's arguments only.
- `STOW_BLESS=1` rewrites the `exit`, `stdout` and `expect` sections of every case from
  what the binary does. Review the diff before keeping it.
