The `baml` wrapper is a BAML application in `baml_src`. It selects and installs
Rust `baml-cli` toolchains and edits configuration. It launches the selected CLI
with inherited standard streams, waits for its status, and then shuts down the
wrapper runtime normally. The CLI itself stays in Rust.

The [matched Rust vs packed-wrapper measurements](BENCHMARKS.md) record startup,
CPU, memory, process/thread counts, binary sizes, export shutdown, and reproduction
instructions after fast telemetry clock startup.

To build an ordinary packed wrapper from this checkout:

```sh
cargo build --bin baml-cli --bin baml-pack-host
./target/debug/baml-cli pack Main --project crates/baml/baml_src \
  --host ./target/debug/baml-pack-host --output ./target/debug/baml-packed
```

Use `NoSelfUpdateMain` for package-manager builds. The release workflow uses the
same pack command with a host built for each target and generates `WrapperVersion`
from the release version. The checked-in version is for direct local packing.

`cargo build -p baml` compiles the same BAML sources into a pack envelope and runs
it through the shared pack host. This supports Cargo development and integration
tests without requiring an installed compiler or nesting a Cargo invocation in a
build script. All wrapper policy lives in BAML.

Installation uses a lock per version, verifies the archive checksum, extracts into
an empty staging directory, checks the toolchain layout, and then publishes it.
Configuration edits hold a separate lock and preserve unrelated TOML and comments.
Windows self-update uses a detached copy of the wrapper to retry executable
replacement after the original process exits. Detachment creates an OS session
or detaches from the Windows console; dropping the subprocess handle does not
terminate the helper.
