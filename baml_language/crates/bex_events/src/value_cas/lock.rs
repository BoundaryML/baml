use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
};

use fs2::FileExt as _;

/// Shared lock held for the complete lifetime of an open pack writer.
#[derive(Debug)]
pub struct WritersLockGuard {
    file: File,
}

impl WritersLockGuard {
    pub fn acquire(store_dir: impl AsRef<Path>) -> io::Result<Self> {
        let file = open_lock_file(store_dir.as_ref().join("writers.lock"))?;
        file.lock_shared()?;
        Ok(Self { file })
    }
}

impl Drop for WritersLockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Exclusive GC serialization and writer-quiescence locks.
#[derive(Debug)]
pub struct GcGuard {
    writers: File,
    gc: File,
}

impl GcGuard {
    /// Non-blocking acquisition. `WouldBlock` means another GC or live pack
    /// writer exists and the CAS pass must be skipped.
    pub fn try_acquire(store_dir: impl AsRef<Path>) -> io::Result<Self> {
        let store_dir = store_dir.as_ref();
        let gc = open_lock_file(store_dir.join("gc.lock"))?;
        gc.try_lock_exclusive()?;
        let writers = open_lock_file(store_dir.join("writers.lock"))?;
        if let Err(error) = writers.try_lock_exclusive() {
            let _ = gc.unlock();
            return Err(error);
        }
        Ok(Self { writers, gc })
    }
}

impl Drop for GcGuard {
    fn drop(&mut self) {
        let _ = self.writers.unlock();
        let _ = self.gc.unlock();
    }
}

fn open_lock_file(path: impl AsRef<Path>) -> io::Result<File> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

#[cfg(test)]
mod tests {
    use std::{fs, io};

    use super::{GcGuard, WritersLockGuard};

    fn temp_store() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "baml-cas-lock-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn gc_skips_while_writer_shared_lock_is_live() {
        let store = temp_store();
        let writer = WritersLockGuard::acquire(&store).unwrap();
        let error = GcGuard::try_acquire(&store).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        drop(writer);
        let gc = GcGuard::try_acquire(&store).unwrap();
        drop(gc);
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn only_one_gc_guard_is_live() {
        let store = temp_store();
        let first = GcGuard::try_acquire(&store).unwrap();
        let error = GcGuard::try_acquire(&store).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        drop(first);
        let second = GcGuard::try_acquire(&store).unwrap();
        drop(second);
        fs::remove_dir_all(store).unwrap();
    }
}
