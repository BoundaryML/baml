//! Git, through the `git` binary: the branch and HEAD, commits made during
//! the session, each file's content when the session started, and whether a
//! path is ignored.
//!
//! Shelling out keeps git's exact semantics (nested `.gitignore` files, global
//! excludes, sparse checkouts) without reimplementing them.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use crate::{
    content::{FileRead, read_path},
    protocol::{CommitInfo, GitState},
};

/// Files larger than this are not read as text.
pub(crate) const MAX_TEXT_BYTES: u64 = 1024 * 1024;

pub(crate) struct Git {
    /// The watched directory; every path this type takes or returns is relative to it.
    root: PathBuf,
}

impl Git {
    /// `None` when `root` is not inside a git work tree.
    pub(crate) fn open(root: &Path) -> Option<Self> {
        let git = Self {
            root: root.to_path_buf(),
        };
        let inside = git.run(&["rev-parse", "--is-inside-work-tree"])?;
        (inside.trim() == "true").then_some(git)
    }

    fn run(&self, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub(crate) fn head(&self) -> Option<String> {
        self.run(&["rev-parse", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    pub(crate) fn branch(&self) -> Option<String> {
        self.run(&["symbolic-ref", "--quiet", "--short", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    pub(crate) fn subject(&self, rev: &str) -> String {
        self.run(&["log", "-1", "--format=%s", rev])
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    /// Commits reachable from HEAD but not from `start`, oldest first.
    pub(crate) fn commits_since(&self, start: &str) -> Vec<CommitInfo> {
        let range = format!("{start}..HEAD");
        let Some(out) = self.run(&["log", "--reverse", "--format=%H%x00%s%x00%ct", &range]) else {
            return Vec::new();
        };
        out.lines()
            .filter_map(|line| {
                let mut parts = line.split('\0');
                let sha = parts.next()?.to_string();
                let subject = parts.next()?.to_string();
                let time = parts.next()?.parse::<u64>().ok()? * 1000;
                Some(CommitInfo { sha, subject, time })
            })
            .collect()
    }

    pub(crate) fn state(&self, start_head: &str) -> Option<GitState> {
        let head = self.head()?;
        Some(GitState {
            branch: self.branch(),
            head_subject: self.subject(&head),
            commits: self.commits_since(start_head),
            head,
            start_head: start_head.to_string(),
        })
    }

    /// What `rel` held at `rev`.
    pub(crate) fn show(&self, rev: &str, rel: &str) -> FileRead {
        // `./` makes the path relative to `-C`, not to the repository root.
        let spec = format!("{rev}:./{rel}");
        let Some(size) = self
            .run(&["cat-file", "-s", &spec])
            .and_then(|s| s.trim().parse::<u64>().ok())
        else {
            return FileRead::Missing;
        };
        if size > FileRead::read_cap(Path::new(rel)) {
            return FileRead::TooLarge { size };
        }
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["cat-file", "blob", &spec])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        match output {
            Ok(output) if output.status.success() => FileRead::from_bytes(output.stdout),
            // Not a blob there (a directory, say): no file.
            _ => FileRead::Missing,
        }
    }

    /// Paths under the root that differ from HEAD (modified, added, deleted,
    /// untracked), with whether each is deleted.
    pub(crate) fn dirty_paths(&self) -> Vec<(String, bool)> {
        let Some(out) = self.run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ]) else {
            return Vec::new();
        };
        let prefix = self
            .run(&["rev-parse", "--show-prefix"])
            .unwrap_or_default();
        let prefix = prefix.trim();
        let mut entries = Vec::new();
        let mut fields = out.split('\0');
        while let Some(entry) = fields.next() {
            if entry.len() < 4 {
                continue;
            }
            let (status, path) = entry.split_at(3);
            // Renames carry the source path as the next field.
            if status.starts_with('R') || status.starts_with('C') {
                fields.next();
            }
            // Porcelain paths are relative to the repository root.
            let Some(rel) = path.strip_prefix(prefix) else {
                continue;
            };
            let deleted = status.contains('D');
            entries.push((rel.to_string(), deleted));
        }
        entries
    }
}

/// Decode file bytes as text, or `None` when they look binary.
pub(crate) fn text_of(bytes: &[u8]) -> Option<String> {
    let sniff = &bytes[..bytes.len().min(8000)];
    if sniff.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Where a file's content at session start comes from: the working tree
/// snapshot taken at startup for files that differed from HEAD, otherwise
/// HEAD itself.
pub(crate) struct Baselines {
    git: Option<Git>,
    start_head: Option<String>,
    /// Files that differed from HEAD at startup, as they were then.
    dirty: HashMap<String, FileRead>,
}

/// Cap on files read into the startup snapshot; beyond it, dirty files fall
/// back to HEAD as their baseline.
const MAX_DIRTY_SNAPSHOT: usize = 4000;

/// Cap on image bytes kept from the startup snapshot. Past it, an image that
/// was there at startup is known to exist but cannot be shown as it was.
const MAX_SNAPSHOT_IMAGE_BYTES: usize = 128 * 1024 * 1024;

impl Baselines {
    pub(crate) fn capture(root: &Path, git: Option<Git>) -> Self {
        let start_head = git.as_ref().and_then(Git::head);
        let mut dirty = HashMap::new();
        let mut image_bytes = 0;
        if let Some(git) = &git {
            for (rel, deleted) in git.dirty_paths().into_iter().take(MAX_DIRTY_SNAPSHOT) {
                let mut read = if deleted {
                    FileRead::Missing
                } else {
                    read_path(&root.join(&rel)).unwrap_or(FileRead::Missing)
                };
                if let FileRead::Binary { image, .. } = &mut read
                    && let Some(kept) = image
                {
                    image_bytes += kept.bytes.len();
                    if image_bytes > MAX_SNAPSHOT_IMAGE_BYTES {
                        *image = None;
                    }
                }
                dirty.insert(rel, read);
            }
        }
        Self {
            git,
            start_head,
            dirty,
        }
    }

    pub(crate) fn start_head(&self) -> Option<&str> {
        self.start_head.as_deref()
    }

    pub(crate) fn git(&self) -> Option<&Git> {
        self.git.as_ref()
    }

    /// What `rel` held when the session started.
    pub(crate) fn baseline(&self, rel: &str) -> FileRead {
        if let Some(snapshot) = self.dirty.get(rel) {
            return snapshot.clone();
        }
        match (self.git.as_ref(), self.start_head.as_deref()) {
            (Some(git), Some(head)) => git.show(head, rel),
            _ => FileRead::Missing,
        }
    }
}

/// Directory names skipped when the root is not a git work tree.
const FALLBACK_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".turbo",
    ".cache",
];

/// Answers "is this path ignored?" with `git check-ignore` running as a
/// coprocess, caching directory answers so a build writing thousands of files
/// under `target/` costs one query.
pub(crate) struct IgnoreFilter {
    checker: Option<CheckIgnore>,
    dirs: HashMap<String, bool>,
}

struct CheckIgnore {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl IgnoreFilter {
    pub(crate) fn new(root: &Path, git: bool) -> Self {
        let checker = git.then(|| CheckIgnore::spawn(root)).flatten();
        Self {
            checker,
            dirs: HashMap::new(),
        }
    }

    /// Whether `rel` (relative to the root, `/`-separated) should be ignored.
    pub(crate) fn is_ignored(&mut self, rel: &str) -> bool {
        let mut dir = String::new();
        let parts: Vec<&str> = rel.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            if *part == ".git" {
                return true;
            }
            dir.push_str(part);
            dir.push('/');
            if self.dir_ignored(&dir) {
                return true;
            }
        }
        match &mut self.checker {
            Some(checker) => checker.ignored(rel).unwrap_or(false),
            None => false,
        }
    }

    fn dir_ignored(&mut self, dir: &str) -> bool {
        if let Some(&known) = self.dirs.get(dir) {
            return known;
        }
        let ignored = match &mut self.checker {
            Some(checker) => checker.ignored(dir).unwrap_or(false),
            None => {
                let name = dir.trim_end_matches('/').rsplit('/').next().unwrap_or("");
                FALLBACK_IGNORED_DIRS.contains(&name)
            }
        };
        self.dirs.insert(dir.to_string(), ignored);
        ignored
    }
}

impl CheckIgnore {
    fn spawn(root: &Path) -> Option<Self> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(root)
            .args([
                "check-ignore",
                "--stdin",
                "-z",
                "--verbose",
                "--non-matching",
            ])
            // Without this git buffers its answers until stdin closes.
            .env("GIT_FLUSH", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdin = child.stdin.take()?;
        let stdout = BufReader::new(child.stdout.take()?);
        Some(Self {
            child,
            stdin,
            stdout,
        })
    }

    /// With `--verbose --non-matching -z`, every query answers with four
    /// NUL-terminated fields: source, line, pattern, path. An empty source
    /// means no pattern matched; a negated pattern (`!foo`) means re-included.
    fn ignored(&mut self, rel: &str) -> Option<bool> {
        self.stdin.write_all(rel.as_bytes()).ok()?;
        self.stdin.write_all(b"\0").ok()?;
        self.stdin.flush().ok()?;
        let mut fields = Vec::with_capacity(4);
        for _ in 0..4 {
            let mut buf = Vec::new();
            self.stdout.read_until(0, &mut buf).ok()?;
            buf.pop();
            fields.push(buf);
        }
        let source = &fields[0];
        let pattern = &fields[2];
        Some(!source.is_empty() && !pattern.starts_with(b"!"))
    }
}

impl Drop for CheckIgnore {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let mut sink = Vec::new();
        let _ = self.stdout.read_to_end(&mut sink);
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    fn repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q"]);
        std::fs::write(dir.path().join(".gitignore"), "target/\n*.log\n!keep.log\n").unwrap();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        dir
    }

    #[test]
    fn ignore_filter_follows_gitignore_including_negation() {
        let dir = repo();
        let mut filter = IgnoreFilter::new(dir.path(), true);
        assert!(filter.is_ignored("target/debug/x.o"));
        assert!(filter.is_ignored("run.log"));
        assert!(!filter.is_ignored("keep.log"));
        assert!(!filter.is_ignored("src/main.rs"));
        assert!(filter.is_ignored(".git/HEAD"));
    }

    fn text(read: FileRead) -> Option<String> {
        match read {
            FileRead::Text(text) => Some(text),
            _ => None,
        }
    }

    #[test]
    fn baselines_prefer_the_startup_snapshot_over_head() {
        let dir = repo();
        std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
        std::fs::write(dir.path().join("new.txt"), "fresh\n").unwrap();
        let baselines = Baselines::capture(dir.path(), Git::open(dir.path()));
        // Dirty at startup: the working tree content, not HEAD's.
        assert_eq!(text(baselines.baseline("a.txt")).as_deref(), Some("two\n"));
        assert_eq!(
            text(baselines.baseline("new.txt")).as_deref(),
            Some("fresh\n")
        );
        // Clean at startup but changed later: HEAD's content.
        assert_eq!(
            text(baselines.baseline(".gitignore")).as_deref(),
            Some("target/\n*.log\n!keep.log\n")
        );
        assert!(matches!(
            baselines.baseline("missing.txt"),
            FileRead::Missing
        ));
    }

    #[test]
    fn baselines_keep_images_from_head_and_the_startup_snapshot() {
        let dir = repo();
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        std::fs::write(dir.path().join("logo.png"), png).unwrap();
        let commit = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .status()
                .unwrap();
            assert!(status.success());
        };
        commit(&["add", "logo.png"]);
        commit(&["commit", "-q", "-m", "logo"]);
        std::fs::write(dir.path().join("shot.png"), png).unwrap();
        let baselines = Baselines::capture(dir.path(), Git::open(dir.path()));
        for rel in ["logo.png", "shot.png"] {
            let FileRead::Binary {
                image: Some(image), ..
            } = baselines.baseline(rel)
            else {
                panic!("{rel} should be an image");
            };
            assert_eq!(image.mime, "image/png");
            assert_eq!(&image.bytes[..], png);
        }
    }

    #[test]
    fn git_state_lists_commits_made_after_the_start() {
        let dir = repo();
        let git = Git::open(dir.path()).unwrap();
        let start = git.head().unwrap();
        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .status()
                .unwrap()
        };
        run(&["add", "b.txt"]);
        run(&["commit", "-q", "-m", "add b"]);
        let state = git.state(&start).unwrap();
        assert_eq!(state.commits.len(), 1);
        assert_eq!(state.commits[0].subject, "add b");
        assert_eq!(state.head_subject, "add b");
    }
}
