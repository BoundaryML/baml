use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use super::{
    format::{BcctHeader, FooterTrailer},
    writer::BcctWriter,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLayout {
    pub project_root: PathBuf,
    pub session_dir: PathBuf,
}

impl SessionLayout {
    #[must_use]
    pub fn new(
        project_root: impl AsRef<Path>,
        started_secs: u64,
        process_euid: [u8; 16],
        engine_id: u64,
    ) -> Self {
        let project_root = project_root.as_ref().to_path_buf();
        let session_name = format!(
            "{started_secs}-{}-e{engine_id}",
            hex_process_euid(process_euid)
        );
        let session_dir = project_root
            .join(".baml")
            .join("sessions")
            .join(session_name);
        Self {
            project_root,
            session_dir,
        }
    }

    #[must_use]
    pub fn meta_path(&self) -> PathBuf {
        self.session_dir.join("session.bamlmeta")
    }

    #[must_use]
    pub fn cct_dir(&self) -> PathBuf {
        self.session_dir.join("cct")
    }

    #[must_use]
    pub fn cct_segment_path(&self, sequence: u32) -> PathBuf {
        self.cct_dir().join(format!("seg-{sequence:06}.bamlseg"))
    }

    #[must_use]
    pub fn flight_dir(&self) -> PathBuf {
        self.session_dir.join("flight")
    }

    #[must_use]
    pub fn raw_dir(&self) -> PathBuf {
        self.session_dir.join("raw")
    }

    pub fn create_dirs(&self) -> io::Result<()> {
        create_dir_all_anchored(&self.cct_dir())?;
        create_dir_all_anchored(&self.flight_dir())?;
        Ok(())
    }

    /// Creates and D2-anchors a new segment header.
    pub fn create_segment(&self, header: &BcctHeader) -> io::Result<BcctWriter<File>> {
        if header.session_seg_seq == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "BCCT segment sequence starts at one",
            ));
        }
        create_dir_all_anchored(&self.cct_dir())?;
        let path = self.cct_segment_path(header.session_seg_seq);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut writer = BcctWriter::create(file, header)?;
        writer.sync_all()?;
        sync_parent_directory(&self.cct_dir())?;
        Ok(writer)
    }
}

pub struct BoundarySnapshot {
    temp_path: PathBuf,
    final_path: PathBuf,
    writer: BcctWriter<File>,
}

impl BoundarySnapshot {
    pub fn create(boundary_dir: &Path, header: &BcctHeader) -> io::Result<Self> {
        create_dir_all_anchored(boundary_dir)?;
        let final_path = boundary_dir.join("cct.bamlcct");
        let temp_path = boundary_dir.join(format!(
            ".cct.bamlcct.tmp-{}-{}",
            std::process::id(),
            header.session_seg_seq
        ));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        let writer = BcctWriter::create(file, header)?;
        Ok(Self {
            temp_path,
            final_path,
            writer,
        })
    }

    pub fn writer_mut(&mut self) -> &mut BcctWriter<File> {
        &mut self.writer
    }

    #[must_use]
    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    /// Seals, fsyncs, atomically renames, and fsyncs the boundary directory.
    pub fn seal_and_commit(mut self) -> io::Result<FooterTrailer> {
        let trailer = self.writer.seal_synced()?;
        drop(self.writer.into_inner());
        anchored_rename(&self.temp_path, &self.final_path)?;
        Ok(trailer)
    }
}

pub fn anchored_rename(from: &Path, to: &Path) -> io::Result<()> {
    if to.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "refusing to replace an existing anchored artifact",
        ));
    }
    fs::rename(from, to)?;
    let parent = to
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    sync_parent_directory(parent)
}

/// Creates every missing directory component and fsyncs its parent before
/// descending, so first-session and first-boundary visibility is D2 as well.
pub fn create_dir_all_anchored(directory: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut cursor = directory;
    while !cursor.exists() {
        missing.push(cursor.to_path_buf());
        cursor = cursor.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "directory has no existing ancestor",
            )
        })?;
    }
    for path in missing.into_iter().rev() {
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        if let Some(parent) = path.parent() {
            sync_parent_directory(parent)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn sync_parent_directory(directory: &Path) -> io::Result<()> {
    File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
pub fn sync_parent_directory(_directory: &Path) -> io::Result<()> {
    Ok(())
}

fn hex_process_euid(bytes: [u8; 16]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
