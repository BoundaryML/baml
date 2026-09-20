//! Read every completed file once, retaining usable files alongside diagnostics.
//! Protobuf detects malformed encodings, not every possible bit corruption: this
//! initial format has no additional checksum. Missing trailing files cannot be
//! inferred without `RecordingEnd`. A readable prefix need not resolve all references.
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use btel_publisher::{CompletionFlags, RecordingId, proto};
use prost::Message;

#[derive(Debug, Eq, PartialEq)]
pub enum ReadIssue {
    InvalidFile { path: PathBuf, reason: String },
    UnfinishedFile(PathBuf),
    MissingSequences { first: u64, last: u64 },
    UnresolvedReference { kind: &'static str, id: u64 },
    FilesAfterRecordingEnd,
}

pub struct RecordingRead {
    pub recording_id: RecordingId,
    /// Sequence order; each successfully decoded/validated file appears once.
    pub files: Vec<proto::RecordingFile>,
    pub issues: Vec<ReadIssue>,
    /// Presence only, not a claim of completeness or final clock validity.
    pub has_recording_end: bool,
}

/// Open a `<recording-id>` directory. No persistent reader state: each call
/// returns a fresh snapshot; do not cumulatively reapply snapshots as deltas.
/// Files published during listing may appear on the next read. Unresolved
/// references can be satisfied by later files, or remain missing after data loss.
pub fn read_directory(directory: &Path) -> io::Result<RecordingRead> {
    let id_text = directory.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let mut bytes = [0; 16];
    if id_text.len() != 32 || !id_text.is_ascii() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a recording ID directory",
        ));
    }
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&id_text[i * 2..i * 2 + 2], 16)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    }
    let id = RecordingId::from_bytes(bytes)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "zero recording ID"))?;
    let mut read = RecordingRead {
        recording_id: id,
        files: Vec::new(),
        issues: Vec::new(),
        has_recording_end: false,
    };
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        if path.extension().is_some_and(|ext| ext == "part") {
            read.issues.push(ReadIssue::UnfinishedFile(path));
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "pb") {
            continue;
        }
        let file = (|| -> Result<proto::RecordingFile, String> {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("invalid sequence filename")?;
            if stem.len() != 20 || !stem.bytes().all(|c| c.is_ascii_digit()) {
                return Err("invalid sequence filename".into());
            }
            let sequence = stem.parse::<u64>().map_err(|e| e.to_string())?;
            if sequence == 0 {
                return Err("zero sequence".into());
            }
            if !fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .is_file()
            {
                return Err("not a regular file".into());
            }
            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            let file = proto::RecordingFile::decode(bytes.as_slice()).map_err(|e| e.to_string())?;
            let header = file.header.as_ref().ok_or("missing recording header")?;
            if header.format_major != btel_settings::encoding::FORMAT_MAJOR {
                return Err("unsupported format major".into());
            }
            if header.recording_id != id.as_bytes() {
                return Err("recording identity mismatch".into());
            }
            if header
                .source_snapshot_id
                .as_ref()
                .is_some_and(|id| id.len() != 32)
            {
                return Err("invalid source snapshot ID".into());
            }
            if file.sequence != sequence {
                return Err("file sequence does not match filename".into());
            }
            validate_definitions(&file)?;
            if let Some(previous) = read.files.first() {
                if previous.header.as_ref().unwrap().source_snapshot_id != header.source_snapshot_id
                {
                    return Err("source snapshot changed within recording".into());
                }
            }
            if let Some(batch) = &file.aggregates {
                for row in &batch.entries {
                    if !valid_node(row.node) || row.count == 0 {
                        return Err("invalid aggregate identity/count".into());
                    }
                }
            }
            if let Some(spans) = &file.spans {
                for section in &spans.sections {
                    if section.thread_id == 0 {
                        return Err("zero thread identity".into());
                    }
                    for record in &section.events {
                        use proto::span_event::Event;
                        match &record.event {
                            Some(
                                Event::FunctionCompletion(done)
                                | Event::LateFunctionCompletion(done),
                            ) => {
                                let late =
                                    matches!(record.event, Some(Event::LateFunctionCompletion(_)));
                                if done.id == 0
                                    || done.parent_id == 0
                                    || !valid_node(done.node)
                                    || CompletionFlags::from_wire(done.completion_flags, late)
                                        .is_none()
                                {
                                    return Err("invalid function completion".into());
                                }
                            }
                            Some(Event::FunctionAnnouncement(entry))
                                if entry.id == 0
                                    || entry.parent_id == 0
                                    || entry.call_path_id == 0 =>
                            {
                                return Err("invalid function announcement".into());
                            }
                            Some(Event::ThreadCompletion(done))
                                if !(1..=3).contains(&done.outcome) =>
                            {
                                return Err("invalid thread outcome".into());
                            }
                            None => return Err("missing span event".into()),
                            _ => {}
                        }
                    }
                }
            }
            Ok(file)
        })();
        match file {
            Ok(file) => read.files.push(file),
            Err(reason) => read.issues.push(ReadIssue::InvalidFile { path, reason }),
        }
    }
    let mut next = 1;
    for file in &read.files {
        if file.sequence > next {
            read.issues.push(ReadIssue::MissingSequences {
                first: next,
                last: file.sequence - 1,
            });
        }
        next = file.sequence.saturating_add(1);
        if read.has_recording_end {
            read.issues.push(ReadIssue::FilesAfterRecordingEnd);
        }
        read.has_recording_end |= file.end.is_some();
    }
    check_references(&mut read);
    Ok(read)
}

fn valid_node(node: u64) -> bool {
    (1..=u64::from(u32::MAX)).contains(&(node >> 1))
}

fn validate_definitions(file: &proto::RecordingFile) -> Result<(), String> {
    if let Some(defs) = &file.definitions {
        if defs
            .functions
            .iter()
            .any(|f| f.function_id == 0 || f.resolution.is_none())
        {
            return Err("invalid function definition".into());
        }
        if defs.call_paths.iter().any(|p| {
            p.call_path_id == 0
                || p.thread_id == 0
                || p.callee_function_id == 0
                || p.visible_caller_function_id == Some(0)
                || !(1..=2).contains(&p.edge)
        }) {
            return Err("invalid call path definition".into());
        }
        if defs
            .threads
            .iter()
            .any(|t| t.thread_id == 0 || t.clock_epoch_id == 0 || t.parent_id == Some(0))
        {
            return Err("invalid thread definition".into());
        }
        if defs.clock_epochs.iter().any(|c| {
            c.epoch_id == 0
                || c.domain_id == 0
                || c.multiplier == 0
                || c.shift > 127
                || !(1..=5).contains(&c.source)
        }) {
            return Err("invalid clock conversion metadata".into());
        }
    }
    if file.clock_states.as_ref().is_some_and(|batch| {
        batch
            .states
            .iter()
            .any(|s| s.epoch_id == 0 || !(1..=5).contains(&s.status))
    }) {
        return Err("invalid clock state".into());
    }
    Ok(())
}

fn check_references(read: &mut RecordingRead) {
    let (mut functions, mut paths, mut threads, mut clocks, mut nodes, mut announcements) = (
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    let mut references = BTreeSet::new();
    for file in &read.files {
        if let Some(defs) = &file.definitions {
            for f in &defs.functions {
                functions.insert(f.function_id);
            }
            for p in &defs.call_paths {
                paths.insert(u64::from(p.call_path_id));
                references.insert(("thread", p.thread_id));
                references.insert(("function", p.callee_function_id));
                if p.parent_call_path_id != 0 {
                    references.insert(("call path", u64::from(p.parent_call_path_id)));
                }
                if let Some(f) = p.visible_caller_function_id {
                    references.insert(("function", f));
                }
            }
            for t in &defs.threads {
                threads.insert(t.thread_id);
                nodes.insert(t.thread_id);
                references.insert(("clock", t.clock_epoch_id));
                if let Some(p) = t.parent_id {
                    references.insert(("node", p));
                }
                if t.spawn_call_path_id != 0 {
                    references.insert(("call path", u64::from(t.spawn_call_path_id)));
                }
            }
            for c in &defs.clock_epochs {
                clocks.insert(c.epoch_id);
            }
        }
        if let Some(batch) = &file.aggregates {
            for delta in &batch.entries {
                references.insert(("call path", delta.node >> 1));
            }
        }
        if let Some(batch) = &file.clock_states {
            for state in &batch.states {
                references.insert(("clock", state.epoch_id));
            }
        }
        if let Some(batch) = &file.spans {
            for section in &batch.sections {
                references.insert(("thread", section.thread_id));
                for event in &section.events {
                    use proto::span_event::Event;
                    match &event.event {
                        Some(Event::FunctionAnnouncement(a)) => {
                            nodes.insert(a.id);
                            announcements.insert(a.id);
                            references.insert(("node", a.parent_id));
                            references.insert(("call path", u64::from(a.call_path_id)));
                        }
                        Some(Event::FunctionCompletion(c) | Event::LateFunctionCompletion(c)) => {
                            nodes.insert(c.id);
                            references.insert(("node", c.parent_id));
                            references.insert(("call path", c.node >> 1));
                            if CompletionFlags::from_wire(c.completion_flags, false)
                                .is_some_and(CompletionFlags::requires_announcement)
                            {
                                references.insert(("announcement", c.id));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    for (kind, id) in references {
        let found = match kind {
            "function" => functions.contains(&id),
            "call path" => paths.contains(&id),
            "thread" => threads.contains(&id),
            "clock" => clocks.contains(&id),
            "node" => nodes.contains(&id),
            "announcement" => announcements.contains(&id),
            _ => unreachable!(),
        };
        if !found {
            read.issues
                .push(ReadIssue::UnresolvedReference { kind, id });
        }
    }
}
