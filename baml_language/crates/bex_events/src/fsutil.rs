//! Durability primitives shared by every artifact writer.
//!
//! One rule everywhere (§6.6/§6.7 barriers): bytes are durable only after
//! the file's own fsync, and a *new name* (create or rename) is durable
//! only after its containing directory's fsync. Rename over an existing
//! name additionally requires the temporary's data to be fsynced first,
//! or a crash can surface the new name with truncated content.

use std::io;
use std::path::Path;

/// Fsync a directory so its entries (creates/renames) are durable.
/// No-op where directories cannot be opened for sync (non-unix).
pub(crate) fn fsync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)?.sync_data()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

/// Durable replace: write `bytes` to `tmp`, fsync it, rename onto `dest`,
/// fsync the destination directory. The classic rename-without-fsync
/// truncation window is closed by the tmp fsync before the rename.
pub(crate) fn write_replace_durable(tmp: &Path, dest: &Path, bytes: &[u8]) -> io::Result<()> {
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(tmp)?;
        file.write_all(bytes)?;
        file.sync_data()?;
    }
    std::fs::rename(tmp, dest)?;
    if let Some(parent) = dest.parent() {
        fsync_dir(parent)?;
    }
    Ok(())
}
