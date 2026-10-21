//! Everything that happened in the watched directory since the server
//! started, and the channel that streams it to pages.
//!
//! Every mutation happens under one lock and is broadcast while the lock is
//! held, so a page that subscribes while holding the lock (see
//! [`Session::subscribe_with_hello`]) sees a `hello` that includes exactly the
//! messages before it and a stream that includes exactly the ones after.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::sync::broadcast;

use crate::{
    content::FileRead,
    git::Baselines,
    media::MediaStore,
    protocol::{
        AgentNote, ChangeKind, FileChange, FlowsMessage, GitState, HelloMessage, MediaRef,
        PROTOCOL_VERSION, RunInfo, ServerMessage, SessionInfo,
    },
};

/// The oldest changes are dropped past this many; baselines roll forward.
const MAX_CHANGES: usize = 5000;

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// What a changed path holds, as far as the session knows: enough to tell
/// whether it exists and whether a write changed it.
#[derive(PartialEq, Eq)]
enum Current {
    Absent,
    Text(String),
    Binary {
        digest: String,
    },
    /// Too large to read; every write counts as a change.
    Unknown,
}

impl Current {
    fn of(read: &FileRead) -> Self {
        match read {
            FileRead::Missing => Self::Absent,
            FileRead::Text(text) => Self::Text(text.clone()),
            FileRead::Binary { digest, .. } => Self::Binary {
                digest: digest.clone(),
            },
            FileRead::TooLarge { .. } => Self::Unknown,
        }
    }
}

pub(crate) struct Session {
    inner: Mutex<State>,
    tx: broadcast::Sender<Arc<str>>,
    baselines: Baselines,
    root: PathBuf,
}

struct State {
    info: SessionInfo,
    seq: u64,
    /// Baseline of every path changed so far (`None`: absent or not text).
    baselines: BTreeMap<String, Option<String>>,
    /// Paths in `baselines` that were images.
    baseline_media: BTreeMap<String, MediaRef>,
    changes: Vec<FileChange>,
    current: HashMap<String, Current>,
    media: MediaStore,
    runs: Vec<RunInfo>,
    run_index: HashMap<String, usize>,
    notes: Vec<AgentNote>,
    flows: BTreeMap<String, FlowsMessage>,
}

impl Session {
    pub(crate) fn new(root: &Path, name: String, baselines: Baselines) -> Arc<Self> {
        let git = baselines
            .git()
            .zip(baselines.start_head())
            .and_then(|(git, head)| git.state(head));
        let (tx, _) = broadcast::channel(1024);
        Arc::new(Self {
            inner: Mutex::new(State {
                info: SessionInfo {
                    root: root.display().to_string(),
                    name,
                    started_at: now_ms(),
                    git,
                },
                seq: 0,
                baselines: BTreeMap::new(),
                baseline_media: BTreeMap::new(),
                changes: Vec::new(),
                current: HashMap::new(),
                media: MediaStore::default(),
                runs: Vec::new(),
                run_index: HashMap::new(),
                notes: Vec::new(),
                flows: BTreeMap::new(),
            }),
            tx,
            baselines,
            root: root.to_path_buf(),
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn start_head(&self) -> Option<&str> {
        self.baselines.start_head()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn send(&self, message: &ServerMessage<'_>) {
        // No receivers is fine: no page is open.
        let _ = self.tx.send(Arc::from(message.to_json()));
    }

    /// A `hello` for the session so far, and a receiver for everything after it.
    pub(crate) fn subscribe_with_hello(&self) -> (String, broadcast::Receiver<Arc<str>>) {
        let state = self.lock();
        let rx = self.tx.subscribe();
        let hello = HelloMessage {
            protocol: PROTOCOL_VERSION,
            session: state.info.clone(),
            baselines: state.baselines.clone(),
            baseline_media: state.baseline_media.clone(),
            changes: state.changes.clone(),
            runs: state.runs.clone(),
            notes: state.notes.clone(),
            flows: state.flows.values().cloned().collect(),
        };
        (ServerMessage::Hello(&hello).to_json(), rx)
    }

    /// Record what `rel` looks like now. Returns whether it was a change (a
    /// write that left the content as it was, or a temporary file that came
    /// and went, is not).
    pub(crate) fn record(&self, rel: &str, read: FileRead) -> bool {
        let mut guard = self.lock();
        let state = &mut *guard;
        let first_touch = !state.baselines.contains_key(rel);
        if first_touch {
            let baseline = self.baselines.baseline(rel);
            if let FileRead::Binary {
                digest,
                image: Some(image),
                ..
            } = &baseline
            {
                let media = state.media.insert(digest, image);
                state.baseline_media.insert(rel.to_string(), media);
            }
            state
                .current
                .insert(rel.to_string(), Current::of(&baseline));
            let text = match baseline {
                FileRead::Text(text) => Some(text),
                _ => None,
            };
            state.baselines.insert(rel.to_string(), text);
        }
        let next = Current::of(&read);
        let previous = state.current.get(rel).unwrap_or(&Current::Absent);
        let existed_before = *previous != Current::Absent;
        if next == *previous && next != Current::Unknown {
            if first_touch && !existed_before {
                // Created and deleted before we looked: it never existed as far as we know.
                state.baselines.remove(rel);
                state.current.remove(rel);
            }
            return false;
        }

        let kind = match (&read, existed_before) {
            (FileRead::Missing, _) => ChangeKind::Deleted,
            (_, true) => ChangeKind::Modified,
            (_, false) => ChangeKind::Created,
        };
        let (after, binary, too_large, size, media) = match read {
            FileRead::Missing => (None, false, false, None, None),
            FileRead::Text(text) => {
                let size = Some(text.len() as u64);
                (Some(text), false, false, size, None)
            }
            FileRead::Binary {
                size,
                digest,
                image,
            } => {
                let media = image.map(|image| state.media.insert(&digest, &image));
                (None, true, false, Some(size), media)
            }
            FileRead::TooLarge { size } => (None, false, true, Some(size), None),
        };

        state.seq += 1;
        let change = FileChange {
            seq: state.seq,
            time: now_ms(),
            path: rel.to_string(),
            kind,
            after,
            binary,
            too_large,
            size,
            media,
        };
        state.current.insert(rel.to_string(), next);
        let baseline = first_touch.then(|| state.baselines.get(rel).cloned().flatten());
        self.send(&ServerMessage::Change {
            change: &change,
            baseline: baseline.as_ref(),
            baseline_media: if first_touch {
                state.baseline_media.get(rel)
            } else {
                None
            },
        });
        state.changes.push(change);
        if state.changes.len() > MAX_CHANGES {
            // Roll the oldest change into its path's baseline.
            let old = state.changes.remove(0);
            match old.media {
                Some(media) => state.baseline_media.insert(old.path.clone(), media),
                None => state.baseline_media.remove(&old.path),
            };
            state.baselines.insert(old.path, old.after);
        }
        true
    }

    /// An image some change or baseline refers to, by content address.
    pub(crate) fn media(&self, id: &str) -> Option<(&'static str, Arc<[u8]>)> {
        let state = self.lock();
        let stored = state.media.get(id)?;
        Some((stored.mime, Arc::clone(&stored.bytes)))
    }

    /// Paths changed so far, for heuristics that relate runs to files.
    pub(crate) fn changed_paths(&self) -> Vec<String> {
        self.lock().baselines.keys().cloned().collect()
    }

    /// Add a run, or update the one with the same id.
    pub(crate) fn upsert_run(&self, run: RunInfo) {
        let mut state = self.lock();
        self.send(&ServerMessage::Run { run: &run });
        match state.run_index.get(&run.id).copied() {
            Some(i) => state.runs[i] = run,
            None => {
                let i = state.runs.len();
                state.run_index.insert(run.id.clone(), i);
                state.runs.push(run);
            }
        }
    }

    pub(crate) fn run(&self, id: &str) -> Option<RunInfo> {
        let state = self.lock();
        state.run_index.get(id).map(|&i| state.runs[i].clone())
    }

    pub(crate) fn add_note(&self, note: AgentNote) {
        let mut state = self.lock();
        self.send(&ServerMessage::Note { note: &note });
        state.notes.push(note);
    }

    /// Update the git state; broadcasts only when something moved.
    pub(crate) fn set_git(&self, git: GitState) {
        let mut state = self.lock();
        if state.info.git.as_ref() == Some(&git) {
            return;
        }
        self.send(&ServerMessage::Git { git: &git });
        state.info.git = Some(git);
    }

    /// Replace a project's workflows; broadcasts only when they changed.
    pub(crate) fn set_flows(&self, flows: FlowsMessage) {
        let mut state = self.lock();
        let unchanged = state.flows.get(&flows.project).is_some_and(|old| {
            serde_json::to_string(&old.workflows).ok()
                == serde_json::to_string(&flows.workflows).ok()
        });
        if unchanged {
            return;
        }
        self.send(&ServerMessage::Flows(&flows));
        state.flows.insert(flows.project.clone(), flows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Baselines;

    fn session() -> (tempfile::TempDir, Arc<Session>) {
        let dir = tempfile::TempDir::new().unwrap();
        let baselines = Baselines::capture(dir.path(), None);
        let session = Session::new(dir.path(), "t".into(), baselines);
        (dir, session)
    }

    #[test]
    fn unchanged_writes_and_transient_files_are_not_changes() {
        let (_dir, session) = session();
        let (_, mut rx) = session.subscribe_with_hello();
        assert!(session.record("a.txt", FileRead::Text("x".into())));
        assert!(!session.record("a.txt", FileRead::Text("x".into())));
        // Created and deleted before we looked: never existed as far as we know.
        assert!(!session.record("tmp.swp", FileRead::Missing));
        assert!(session.record("a.txt", FileRead::Missing));
        let first = rx.try_recv().unwrap();
        assert!(first.contains(r#""kind":"created""#), "{first}");
        assert!(first.contains(r#""baseline":null"#), "{first}");
        let second = rx.try_recv().unwrap();
        assert!(second.contains(r#""kind":"deleted""#), "{second}");
        assert!(!second.contains("baseline"), "{second}");
    }

    #[test]
    fn images_are_stored_once_and_rewriting_the_same_bytes_is_not_a_change() {
        let (_dir, session) = session();
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        let (_, mut rx) = session.subscribe_with_hello();
        assert!(session.record("a.png", FileRead::from_bytes(png.clone())));
        assert!(!session.record("a.png", FileRead::from_bytes(png.clone())));
        let created = rx.try_recv().unwrap();
        let id = crate::media::digest(&png);
        assert!(
            created.contains(&format!(
                r#""media":{{"url":"/api/agent-activity/media/{id}","mime":"image/png"}}"#
            )),
            "{created}"
        );
        let (mime, bytes) = session.media(&id).unwrap();
        assert_eq!((mime, &bytes[..]), ("image/png", &png[..]));

        // Other binaries are compared by content too, without being stored.
        assert!(session.record("a.bin", FileRead::from_bytes(vec![0, 1])));
        assert!(!session.record("a.bin", FileRead::from_bytes(vec![0, 1])));
        assert!(session.record("a.bin", FileRead::from_bytes(vec![0, 2])));
    }

    #[test]
    fn hello_carries_the_history_and_the_stream_continues_after_it() {
        let (_dir, session) = session();
        session.record("a.txt", FileRead::Text("1".into()));
        let (hello, mut rx) = session.subscribe_with_hello();
        assert!(hello.contains(r#""type":"hello""#));
        assert!(hello.contains(r#""path":"a.txt""#));
        session.record("a.txt", FileRead::Text("2".into()));
        let next = rx.try_recv().unwrap();
        assert!(next.contains(r#""kind":"modified""#), "{next}");
    }
}
