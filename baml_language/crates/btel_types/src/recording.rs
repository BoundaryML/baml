/// The UUID scope shared by a recording and its public span identities.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecordingId([u8; 16]);

impl RecordingId {
    pub fn generate() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Option<Self> {
        (bytes != [0; 16]).then_some(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// The OS process a recording's engine ran in. Every engine of one process
/// shares `process_id`; the rest describes the process for debugging.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessInfo {
    pub process_id: [u8; 16],
    pub baml_version: String,
    /// `cli`, `lsp`, `pack`, `python`, ...
    pub host: String,
    pub command: Vec<String>,
    pub started_at_unix_ns: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessStatus {
    Success,
    Error,
    Panicked,
}

/// How a process ended, as its host decided before shutting engines down.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessExit {
    pub status: ProcessStatus,
    pub at_unix_ns: i64,
}

/// Written by the host once it knows the process is exiting, read when the
/// recording ends. Engines shut down for any other reason leave it empty.
#[derive(Debug, Default)]
pub struct ProcessExitSlot(std::sync::Mutex<Option<ProcessExit>>);

impl ProcessExitSlot {
    pub fn set(&self, exit: ProcessExit) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(exit);
    }

    pub fn get(&self) -> Option<ProcessExit> {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
