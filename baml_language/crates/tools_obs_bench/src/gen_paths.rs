use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

const MAX_PATHS: u32 = 4096;
const MAX_TOTAL_CALLS: u64 = 100_000_000;

#[derive(Debug, Serialize)]
pub(crate) struct GeneratedPaths {
    schema_version: u32,
    output: String,
    paths: u32,
    total_calls: u64,
    repetitions: u64,
    remainder: u32,
    source_bytes: usize,
}

pub(crate) fn generate(output: &Path, paths: u32, total_calls: u64) -> Result<GeneratedPaths> {
    if paths == 0 || paths > MAX_PATHS {
        bail!("paths must be in 1..={MAX_PATHS}");
    }
    if total_calls == 0 || total_calls > MAX_TOTAL_CALLS {
        bail!("total_calls must be in 1..={MAX_TOTAL_CALLS}");
    }
    if total_calls < u64::from(paths) {
        bail!("total_calls must be at least paths so every context is exercised");
    }
    let repetitions = total_calls / u64::from(paths);
    let remainder = u32::try_from(total_calls % u64::from(paths)).unwrap_or(0);
    let source = render(paths, repetitions, remainder)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(output, &source).with_context(|| format!("write {}", output.display()))?;
    Ok(GeneratedPaths {
        schema_version: 1,
        output: output.display().to_string(),
        paths,
        total_calls,
        repetitions,
        remainder,
        source_bytes: source.len(),
    })
}

fn render(paths: u32, repetitions: u64, remainder: u32) -> Result<String> {
    let mut source = String::from(
        "// Generated deterministically by `obs-bench gen-paths`.\n\
         // Each PathN function creates one distinct dynamic calling context.\n\n",
    );
    for path in 0..paths {
        writeln!(
            source,
            "function Leaf{path}(value: int) -> int {{ value + {path} }}"
        )?;
        writeln!(
            source,
            "function Path{path}(value: int) -> int {{ Leaf{path}(value) }}\n"
        )?;
    }
    writeln!(
        source,
        "function GeneratedPaths() -> int {{\n  let checksum = 0;\n  let round = 0;\n  while (round < {repetitions}) {{"
    )?;
    for path in 0..paths {
        writeln!(source, "    checksum = checksum + Path{path}(round);")?;
    }
    writeln!(source, "    round = round + 1;\n  }};")?;
    for path in 0..remainder {
        writeln!(source, "  checksum = checksum + Path{path}(round);")?;
    }
    writeln!(source, "  checksum\n}}")?;
    Ok(source)
}

pub(crate) fn default_output(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join("workloads/paths/generated_paths.baml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_is_deterministic_and_names_every_path() {
        let first = render(4, 10, 2).unwrap();
        let second = render(4, 10, 2).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("function Path3"));
        assert!(first.contains("Path1(round)"));
    }
}
