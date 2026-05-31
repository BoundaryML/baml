//! Regenerates the checked-in builtin / IO source files.
//! Run with `cargo run -p tools_rustgen`.

// A CLI tool reporting what it wrote — stdout/stderr is the intended output.
#[allow(clippy::print_stdout, clippy::print_stderr)]
fn main() -> anyhow::Result<()> {
    let written = tools_rustgen::write_all()?;
    for path in &written {
        println!("wrote {}", path.display());
    }
    eprintln!("tools_rustgen: wrote {} generated file(s)", written.len());
    Ok(())
}
