//! Reads the agent's own transcript for what the file system cannot show:
//! what it said (captions) and what it ran.
//!
//! Claude Code writes one JSON line per event to
//! `~/.claude/projects/<cwd with every non-alphanumeric byte as '-'>/<session>.jsonl`.
//! Sessions started in the watched directory or below it are followed; each
//! line carries its `cwd`, which confirms it. Text blocks from the assistant
//! become notes, `Bash` tool calls become runs, and their tool results finish
//! them. Only lines written after the server started are read.

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use serde_json::Value;

use crate::{
    protocol::{AgentNote, RunFrame, RunInfo, RunKind, RunStatus},
    session::Session,
};

const POLL: Duration = Duration::from_millis(700);
/// Notes longer than this are cut; a caption is not the place for a report.
const MAX_NOTE_CHARS: usize = 1200;
const LOG_TAIL_LINES: usize = 40;

/// Claude Code's directory name for a working directory.
pub(crate) fn claude_project_dir_name(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

pub(crate) fn spawn(session: Arc<Session>) -> std::io::Result<()> {
    let Some(home) = dirs::home_dir() else {
        return Ok(());
    };
    let base = home.join(".claude").join("projects");
    std::thread::Builder::new()
        .name("agent-watch-transcripts".into())
        .spawn(move || {
            let mut tailer = Tailer::new(session, base);
            loop {
                tailer.poll();
                std::thread::sleep(POLL);
            }
        })?;
    Ok(())
}

struct Tailer {
    session: Arc<Session>,
    base: PathBuf,
    prefix: String,
    started: SystemTime,
    offsets: HashMap<PathBuf, u64>,
    functions: FunctionIndex,
}

impl Tailer {
    fn new(session: Arc<Session>, base: PathBuf) -> Self {
        let prefix = claude_project_dir_name(session.root());
        let functions = FunctionIndex::new(session.root());
        Self {
            session,
            base,
            prefix,
            started: SystemTime::now(),
            offsets: HashMap::new(),
            functions,
        }
    }

    fn poll(&mut self) {
        let Ok(dirs) = std::fs::read_dir(&self.base) else {
            return;
        };
        for dir in dirs.flatten() {
            let name = dir.file_name().to_string_lossy().into_owned();
            // Sessions in the root or any directory below it.
            if name != self.prefix && !name.starts_with(&format!("{}-", self.prefix)) {
                continue;
            }
            let Ok(files) = std::fs::read_dir(dir.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().is_some_and(|e| e == "jsonl") {
                    self.read_new_lines(&path);
                }
            }
        }
    }

    fn read_new_lines(&mut self, path: &Path) {
        let Ok(meta) = std::fs::metadata(path) else {
            return;
        };
        let len = meta.len();
        let offset = *self.offsets.entry(path.to_path_buf()).or_insert_with(|| {
            // A transcript last written before we started holds only history.
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if modified < self.started { len } else { 0 }
        });
        if len <= offset {
            if len < offset {
                self.offsets.insert(path.to_path_buf(), len);
            }
            return;
        }
        let Ok(mut file) = File::open(path) else {
            return;
        };
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return;
        }
        let mut buf = Vec::with_capacity(usize::try_from(len - offset).unwrap_or(0));
        if file.take(len - offset).read_to_end(&mut buf).is_err() {
            return;
        }
        // Only whole lines; a partly written line is read on the next poll.
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else {
            return;
        };
        self.offsets
            .insert(path.to_path_buf(), offset + end as u64 + 1);
        for line in buf[..end].split(|&b| b == b'\n') {
            if let Ok(value) = serde_json::from_slice::<Value>(line) {
                self.handle(&value);
            }
        }
    }

    fn handle(&mut self, line: &Value) {
        if line.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let Some(cwd) = line.get("cwd").and_then(Value::as_str) else {
            return;
        };
        if !Path::new(cwd).starts_with(self.session.root()) {
            return;
        }
        let time = line
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(iso_to_ms)
            .unwrap_or_else(crate::session::now_ms);
        let uuid = line.get("uuid").and_then(Value::as_str).unwrap_or("");
        let Some(blocks) = line.pointer("/message/content").and_then(Value::as_array) else {
            return;
        };
        match line.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for (i, block) in blocks.iter().enumerate() {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let text = block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .trim();
                            if !text.is_empty() {
                                self.session.add_note(AgentNote {
                                    id: format!("{uuid}:{i}"),
                                    time,
                                    text: truncate_chars(text, MAX_NOTE_CHARS),
                                    agent: "claude-code".into(),
                                });
                            }
                        }
                        Some("tool_use")
                            if block.get("name").and_then(Value::as_str) == Some("Bash") =>
                        {
                            let Some(id) = block.get("id").and_then(Value::as_str) else {
                                continue;
                            };
                            let command = block
                                .pointer("/input/command")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            self.start_run(id, command, time);
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for block in blocks {
                    if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                        continue;
                    }
                    let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                        continue;
                    };
                    let failed = block
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    self.finish_run(id, &result_text(block.get("content")), failed, time);
                }
            }
            _ => {}
        }
    }

    fn start_run(&mut self, id: &str, command: &str, time: u64) {
        let (kind, target_name) = classify(command);
        let target = target_name.as_deref().and_then(|name| {
            let (path, line) = self.functions.find(name)?;
            Some(RunFrame {
                name: name.to_string(),
                path,
                line: Some(line),
                duration_ms: None,
                llm: None,
            })
        });
        let touches = match &target {
            Some(t) => vec![t.path.clone()],
            None => mentioned_paths(command, &self.session.changed_paths()),
        };
        self.session.upsert_run(RunInfo {
            id: id.to_string(),
            kind,
            command: command
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(300)
                .collect(),
            status: RunStatus::Running,
            started_at: time,
            ended_at: None,
            target,
            frames: None,
            touches,
            input: None,
            output: None,
            error: None,
            log: None,
        });
    }

    fn finish_run(&mut self, id: &str, text: &str, failed: bool, time: u64) {
        let Some(mut run) = self.session.run(id) else {
            return;
        };
        run.status = if failed {
            RunStatus::Failed
        } else {
            RunStatus::Passed
        };
        run.ended_at = Some(time);
        if run.kind == RunKind::BamlRun {
            run.output = json_tail(text);
            if let Some(target) = &run.target {
                run.frames = Some(vec![target.clone()]);
            }
        }
        if run.output.is_none() {
            let lines: Vec<&str> = text.lines().collect();
            run.log = Some(lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n"));
        }
        self.session.upsert_run(run);
    }
}

/// The kind of a shell command, and for `baml run` the function it runs.
pub(crate) fn classify(command: &str) -> (RunKind, Option<String>) {
    let words = shell_words(command);
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    for (i, w) in words.iter().enumerate() {
        let program = w.rsplit('/').next().unwrap_or(w);
        if program != "baml" && program != "baml-cli" {
            continue;
        }
        match words.get(i + 1).copied() {
            Some("run") => return (RunKind::BamlRun, run_target(&words[i + 2..])),
            Some("test") => return (RunKind::BamlTest, None),
            _ => {}
        }
    }
    (RunKind::Command, None)
}

/// Split a command line into words the way a POSIX shell would for simple
/// commands: whitespace separates, single and double quotes group. Enough to
/// find a `baml run` target; not a full shell parser.
fn shell_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in command.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            None => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

/// `baml run` options that take a value (see `baml_cli::run_command::RunArgs`).
const RUN_VALUE_FLAGS: &[&str] = &[
    "--project",
    "--from",
    "--file",
    "-e",
    "--expression",
    "--output-format",
    "--log",
    "--log-file",
    "-F",
    "--features",
];

/// The function `baml run <args>` runs: its positional target or `--function`.
fn run_target(args: &[&str]) -> Option<String> {
    let mut rest = args
        .iter()
        .copied()
        .take_while(|w| !matches!(*w, "--" | "&&" | "||" | "|" | ";"));
    while let Some(word) = rest.next() {
        if word == "-f" || word == "--function" {
            return rest.next().map(str::to_string);
        }
        if let Some(name) = word.strip_prefix("--function=") {
            return Some(name.to_string());
        }
        if RUN_VALUE_FLAGS.contains(&word) {
            rest.next();
        } else if !word.starts_with('-') {
            return Some(word.to_string());
        }
    }
    None
}

/// Changed paths whose file name appears in `command`.
fn mentioned_paths(command: &str, changed: &[String]) -> Vec<String> {
    changed
        .iter()
        .filter(|p| {
            let name = p.rsplit('/').next().unwrap_or(p);
            name.len() > 3 && command.contains(name)
        })
        .take(8)
        .cloned()
        .collect()
}

fn result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The JSON value a command printed last, if its output ends with one.
fn json_tail(text: &str) -> Option<Value> {
    let lines: Vec<&str> = text.lines().collect();
    for start in 0..lines.len() {
        let first = lines[start].trim_start();
        if !(first.starts_with('{') || first.starts_with('[')) {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&lines[start..].join("\n")) {
            return Some(value);
        }
    }
    None
}

fn truncate_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// `2026-09-23T05:36:46.968Z` → Unix milliseconds.
pub(crate) fn iso_to_ms(stamp: &str) -> Option<u64> {
    let (date, time) = stamp.split_once('T')?;
    let mut ymd = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (ymd.next()?.ok()?, ymd.next()?.ok()?, ymd.next()?.ok()?);
    let time = time.trim_end_matches('Z');
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut clock = hms.split(':').map(str::parse::<i64>);
    let (hours, minutes, seconds) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    let millis: i64 = format!("{frac:0<3}")[..3].parse().ok()?;
    // Days from civil (Howard Hinnant's algorithm).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let total = ((days * 86_400 + hours * 3600 + minutes * 60 + seconds) * 1000) + millis;
    u64::try_from(total).ok()
}

/// Finds where a BAML function is declared, for runs that name one.
struct FunctionIndex {
    root: PathBuf,
    cache: HashMap<String, Option<(String, u32)>>,
}

impl FunctionIndex {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            cache: HashMap::new(),
        }
    }

    fn find(&mut self, name: &str) -> Option<(String, u32)> {
        if let Some(hit) = self.cache.get(name) {
            return hit.clone();
        }
        let needle = format!("function {name}(");
        let found = ignore::WalkBuilder::new(&self.root)
            .build()
            .flatten()
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("baml"))
            })
            .find_map(|e| {
                let text = std::fs::read_to_string(e.path()).ok()?;
                let line = text
                    .lines()
                    .position(|l| l.trim_start().starts_with(&needle))?;
                let rel = crate::watcher::relative(&self.root, e.path())?;
                Some((rel, u32::try_from(line).ok()?))
            });
        self.cache.insert(name.to_string(), found.clone());
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_directory_names_replace_every_non_alphanumeric_byte() {
        assert_eq!(
            claude_project_dir_name(Path::new("/Users/me/projects/baml2/baml_language")),
            "-Users-me-projects-baml2-baml-language"
        );
    }

    #[test]
    fn baml_run_commands_name_their_function() {
        assert_eq!(
            classify("cd proj && baml run ExtractResume --args-file x.json"),
            (RunKind::BamlRun, Some("ExtractResume".into()))
        );
        assert_eq!(
            classify("baml-cli run --project p main"),
            (RunKind::BamlRun, Some("main".into()))
        );
        assert_eq!(
            classify("baml run --output-format json -f Rank -- Rank --job x"),
            (RunKind::BamlRun, Some("Rank".into()))
        );
        assert_eq!(classify("baml run -e '1 + 1'"), (RunKind::BamlRun, None));
        assert_eq!(classify("baml test"), (RunKind::BamlTest, None));
        assert_eq!(classify("cargo nextest run -p x"), (RunKind::Command, None));
    }

    #[test]
    fn iso_timestamps_parse_to_unix_millis() {
        assert_eq!(iso_to_ms("1970-01-01T00:00:01.5Z"), Some(1500));
        assert_eq!(
            iso_to_ms("2026-09-23T05:36:46.968Z"),
            Some(1_790_141_806_968)
        );
    }

    #[test]
    fn transcript_lines_become_notes_and_runs() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("baml_src")).unwrap();
        std::fs::write(
            root.join("baml_src/a.baml"),
            "class R {\n  x: int,\n}\n\nfunction Extract(t: string) -> R {\n  client: C\n}\n",
        )
        .unwrap();
        let session = Session::new(
            &root,
            "t".into(),
            crate::git::Baselines::capture(&root, None),
        );
        let (_, mut rx) = session.subscribe_with_hello();
        let mut tailer = Tailer::new(session, root.join("unused"));
        let cwd = root.display().to_string();
        tailer.handle(&serde_json::json!({
            "type": "assistant", "cwd": cwd, "uuid": "u1", "timestamp": "2026-09-23T05:00:00.000Z",
            "message": { "content": [
                { "type": "text", "text": "Running Extract on the sample." },
                { "type": "tool_use", "id": "t1", "name": "Bash",
                  "input": { "command": "baml run Extract --args-file s.json" } }
            ]}
        }));
        tailer.handle(&serde_json::json!({
            "type": "user", "cwd": cwd, "timestamp": "2026-09-23T05:00:03.000Z",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "t1", "is_error": false,
                  "content": "Running Extract\n{\"x\": 3}" }
            ]}
        }));
        // Another directory's session is not ours.
        tailer.handle(&serde_json::json!({
            "type": "assistant", "cwd": "/elsewhere", "uuid": "u2",
            "message": { "content": [{ "type": "text", "text": "not here" }] }
        }));

        let note = rx.try_recv().unwrap();
        assert!(
            note.contains(r#""text":"Running Extract on the sample.""#),
            "{note}"
        );
        let started = rx.try_recv().unwrap();
        assert!(started.contains(r#""status":"running""#), "{started}");
        assert!(
            started.contains(r#""path":"baml_src/a.baml","line":4"#),
            "{started}"
        );
        let finished = rx.try_recv().unwrap();
        assert!(finished.contains(r#""status":"passed""#), "{finished}");
        assert!(finished.contains(r#""output":{"x":3}"#), "{finished}");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn json_tail_finds_a_trailing_value() {
        let out = "Running ExtractResume\n{\n  \"name\": \"Ada\"\n}\n";
        assert_eq!(json_tail(out), Some(serde_json::json!({ "name": "Ada" })));
        assert_eq!(json_tail("no json here"), None);
    }
}
