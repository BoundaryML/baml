//! Embed the stdlib slice this crate's transient runtime compiler splices in.
//!
//! Cargo keys `OUT_DIR` and reruns this script with the exact compiler
//! dependency graph, so the embedded bytes cannot outlive the build that
//! produced their `Program`/`PackageInterface` layouts.
//!
//! The producer in `baml_db::stdlib_prefix::runtime` is shared with the
//! standalone host tool. Cargo calls it directly, without building or launching
//! an additional generator executable.

fn main() -> std::io::Result<()> {
    let out_dir = std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts");
    baml_db::stdlib_prefix::runtime::generate(std::path::Path::new(&out_dir))
}
