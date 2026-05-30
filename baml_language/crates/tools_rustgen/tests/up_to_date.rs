//! Fails if the checked-in generated files are stale relative to the codegen.
//! This is the freshness guarantee that replaces the old `build.rs`: it runs in
//! the normal `cargo test` / nextest suite, so CI (and you, locally) catch drift.

/// Normalize line endings so the comparison is eol-agnostic: on Windows CI the
/// files may be checked out as CRLF while `render_all()` always emits LF. Line
/// endings don't affect what the compiler ingests, so they must not count as drift.
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn generated_files_are_up_to_date() {
    let base = tools_rustgen::crates_dir();
    let mut stale = Vec::new();

    for gf in tools_rustgen::render_all().expect("codegen failed") {
        let path = base.join(gf.rel_path);
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        if lf(&on_disk) != lf(&gf.contents) {
            stale.push(gf.rel_path);
        }
    }

    assert!(
        stale.is_empty(),
        "generated files are stale: {stale:?}\n\
         run `cargo run -p tools_rustgen` and commit the result",
    );
}
