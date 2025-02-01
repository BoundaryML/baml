pub struct File {
    /// The path of the file.
    pub path: String,

    /// The unix permissions of the file. Only supported on unix systems. Always `None` on Windows
    /// or when the file has been deleted.
    pub permissions: Option<u32>,

    /// The file revision. A file has changed if the revisions don't compare equal.
    pub revision: FileRevision,

    /// The status of the file.
    ///
    /// Salsa doesn't support deleting inputs. The only way to signal dependent queries that
    /// the file has been deleted is to change the status to `Deleted`.
    pub status: FileStatus,

}

pub enum FileStatus {
    Exists,
    IsADirectory,
    NotFound,
}

pub struct FileRevision(u128);

impl FileRevision {
    pub fn new(value: u128) -> Self {
        Self(value)
    }

    pub fn now() -> Self {
        Self::from(filetime::FileTime::now())
    }

    pub const fn zero() -> Self {
        Self(0)
    }

    #[must_use]
    pub fn as_u128(self) -> u128 {
        self.0
    }
}

impl From<u128> for FileRevision {
    fn from(value: u128) -> Self {
        FileRevision(value)
    }
}

impl From<u64> for FileRevision {
    fn from(value: u64) -> Self {
        FileRevision(u128::from(value))
    }
}

impl From<filetime::FileTime> for FileRevision {
    fn from(value: filetime::FileTime) -> Self {
        let seconds = value.seconds() as u128;
        let seconds = seconds << 64;
        let nanos = u128::from(value.nanoseconds());

        FileRevision(seconds | nanos)
    }
}