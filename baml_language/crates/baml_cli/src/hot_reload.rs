//! `baml run --hot`: patch the running program whenever its source changes.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use bex_engine::{BexEngine, HotswapError};

use crate::paint::Painter;

const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// What `--hot` watches, and the source the running program was compiled
/// from, keyed as `HotFiles` reads it: the watcher's first snapshot, so a save
/// made while the program was starting is still applied.
pub(crate) struct HotWatch {
    pub(crate) files: HotFiles,
    pub(crate) built: BTreeMap<String, String>,
}

pub(crate) enum HotFiles {
    /// A project, by its root: the files project discovery finds, ignore files
    /// and source root included, so a patch compiles what the program was
    /// built from.
    Project(PathBuf),
    /// One standalone file (`--file`).
    File(PathBuf),
}

/// How a standalone file is keyed in the baseline: by its name, the way
/// `HotFiles::File` reads it.
pub(crate) fn file_key(path: &Path) -> String {
    path.file_name()
        .expect("a standalone file has a name")
        .to_string_lossy()
        .into_owned()
}

/// The sources as they stand on disk, keyed as the baseline keys them:
/// relative to the project root, or by file name for a standalone file.
/// `None` when a file cannot be read, such as while an editor replaces it.
fn read_sources(files: &HotFiles) -> Option<BTreeMap<String, String>> {
    match files {
        HotFiles::Project(root) => {
            baml_db::discover_baml_files(&baml_db::project_source_root(root))
                .into_iter()
                .map(|path| {
                    let text = std::fs::read_to_string(&path).ok()?;
                    let relative = path.strip_prefix(root).unwrap_or(&path);
                    Some((relative.to_string_lossy().into_owned(), text))
                })
                .collect()
        }
        HotFiles::File(path) => std::fs::read_to_string(path)
            .ok()
            .map(|text| BTreeMap::from([(file_key(path), text)])),
    }
}

/// One cargo-style status line on stderr, beside the program's own output:
/// the verb right-aligned and painted by `paint`, then `message`, then
/// `details`, each on its own line under the message.
#[allow(
    clippy::print_stderr,
    reason = "swap reports go to stderr, beside the running program's own output"
)]
fn report(verb: &str, paint: impl Fn(String) -> String, message: &str, details: &[String]) {
    let verb = paint(format!("{verb:>12}"));
    let mut out = format!("{verb} {message}");
    for detail in details {
        for line in detail.lines() {
            out.push_str(&format!("\n{:12} {line}", ""));
        }
    }
    eprintln!("{out}");
}

fn duration(elapsed: std::time::Duration) -> String {
    let millis = elapsed.as_millis();
    if millis < 1000 {
        format!("{millis}ms")
    } else {
        format!("{:.1}s", elapsed.as_secs_f64())
    }
}

/// A rejection's reasons, each one line, without the shared "which needs a
/// restart" the header already says.
fn reasons(problems: &[String]) -> Vec<String> {
    problems
        .iter()
        .map(|problem| format!("- {}", problem.replace(", which needs a restart", "")))
        .collect()
}

async fn patch(engine: &Arc<BexEngine>, sources: &BTreeMap<String, String>) {
    let files = sources
        .iter()
        .map(|(path, text)| (path.clone(), text.clone()))
        .collect();
    let started = std::time::Instant::now();
    let result = engine.hotswap_patch(files).await;
    let painter = Painter::stderr();
    let took = painter.dim(format!("({})", duration(started.elapsed())));
    let failed = |verb| painter.bad(verb);
    // On any failure the program keeps running the last good code.
    match result {
        // A save that swaps nothing, such as a comment, says nothing.
        Ok(names) if names.is_empty() => {}
        Ok(names) => report(
            "Swapped",
            |verb| painter.good(verb),
            &format!("{} {took}", names.join(", ")),
            &[],
        ),
        Err(HotswapError::Rejected(problems)) => report(
            "Rejected",
            failed,
            &format!("save needs a restart {took}"),
            &reasons(&problems),
        ),
        Err(HotswapError::Compile(message)) => report(
            "Failed",
            failed,
            &format!("save does not compile {took}"),
            &[message],
        ),
        Err(error) => report("Failed", failed, &format!("{error} {took}"), &[]),
    }
}

/// Watch until aborted.
pub(crate) fn spawn(
    rt: &tokio::runtime::Runtime,
    engine: Arc<BexEngine>,
    watch: HotWatch,
) -> tokio::task::JoinHandle<()> {
    rt.spawn(async move {
        let HotWatch { files, built } = watch;
        if !crate::reporter::quiet() {
            let what = match &files {
                HotFiles::Project(_) => match built.len() {
                    1 => "1 file".to_string(),
                    count => format!("{count} files"),
                },
                HotFiles::File(path) => file_key(path),
            };
            let painter = Painter::stderr();
            report(
                "Watching",
                |verb| painter.accent(verb),
                &format!(
                    "{what} {}",
                    painter.dim("(each save swaps into the running program)")
                ),
                &[],
            );
        }
        let mut last = built;
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            let Some(current) = read_sources(&files) else {
                continue;
            };
            if current != last {
                last.clone_from(&current);
                patch(&engine, &current).await;
            }
        }
    })
}
