//! A finite snapshot of completed recording files. Files published after the
//! listing belong to the next discovery; `.part` files are never complete.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use btel_recorder::RecordingId;

use crate::layout::SourceLayout;

/// Filesystem identity used to skip unchanged files without reading them.
/// Metadata cannot detect every in-place rewrite that preserves it; a full
/// verification pass hashes contents instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    /// Modification time in nanoseconds since the Unix epoch.
    pub modified_ns: i128,
    pub device: u64,
    pub inode: u64,
}

impl FileStamp {
    pub fn of(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                size: metadata.len(),
                modified_ns: i128::from(metadata.mtime()) * 1_000_000_000
                    + i128::from(metadata.mtime_nsec()),
                device: metadata.dev(),
                inode: metadata.ino(),
            }
        }
        #[cfg(not(unix))]
        {
            let modified_ns = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |elapsed| elapsed.as_nanos() as i128);
            Self {
                size: metadata.len(),
                modified_ns,
                device: 0,
                inode: 0,
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiscoveredFile {
    pub sequence: u64,
    pub path: PathBuf,
    pub stamp: FileStamp,
}

#[derive(Clone, Debug)]
pub struct DiscoveredRecording {
    pub id: RecordingId,
    pub directory: PathBuf,
    /// Completed files in sequence order.
    pub files: Vec<DiscoveredFile>,
    /// `.part` files present at listing time: writes in progress or abandoned.
    pub partial_files: usize,
    /// Entries that are not completed files or partial files.
    pub ignored: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct Discovery {
    /// Recordings in ID order.
    pub recordings: Vec<DiscoveredRecording>,
    /// Whether `recordings/` existed at all.
    pub source_exists: bool,
    /// Entries of `recordings/` that are not recording directories.
    pub ignored: Vec<PathBuf>,
}

impl Discovery {
    pub fn file_count(&self) -> usize {
        self.recordings.iter().map(|r| r.files.len()).sum()
    }
}

/// List every recording directory and its completed files once. Reads
/// directory entries and metadata only; never file contents.
pub fn discover(layout: &SourceLayout) -> io::Result<Discovery> {
    let entries = match fs::read_dir(&layout.recordings) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Discovery::default());
        }
        Err(error) => return Err(error),
    };
    let mut discovery = Discovery {
        source_exists: true,
        ..Discovery::default()
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let id = entry
            .file_name()
            .to_str()
            .and_then(btel_file::parse_recording_id);
        match id {
            Some(id) if entry.file_type()?.is_dir() => {
                discovery.recordings.push(discover_recording(id, path)?);
            }
            _ => discovery.ignored.push(path),
        }
    }
    discovery
        .recordings
        .sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
    Ok(discovery)
}

fn discover_recording(id: RecordingId, directory: PathBuf) -> io::Result<DiscoveredRecording> {
    let mut recording = DiscoveredRecording {
        id,
        directory,
        files: Vec::new(),
        partial_files: 0,
        ignored: Vec::new(),
    };
    let entries = match fs::read_dir(&recording.directory) {
        Ok(entries) => entries,
        // Removed between listings: an empty recording for this snapshot.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(recording),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_str().unwrap_or_default();
        if Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("part"))
        {
            recording.partial_files += 1;
            continue;
        }
        let Some(sequence) = btel_file::parse_sequence_file_name(name) else {
            recording.ignored.push(path);
            continue;
        };
        // Symlinks and other non-regular entries are never recording files.
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                recording.ignored.push(path);
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        recording.files.push(DiscoveredFile {
            sequence,
            path,
            stamp: FileStamp::of(&metadata),
        });
    }
    recording.files.sort_by_key(|file| file.sequence);
    Ok(recording)
}

/// Hex rendering used for recording directories and public identifiers.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("write to String");
    }
    text
}

pub fn recording_directory(layout: &SourceLayout, id: RecordingId) -> PathBuf {
    layout.recordings.join(hex(id.as_bytes()))
}

pub fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
