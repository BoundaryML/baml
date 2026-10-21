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
    /// The host finalized its SDK lifetime without reporting an exit outcome.
    Unknown,
    Success,
    Error,
    Panicked,
}

/// The end of a host's BAML lifetime, reported before shutting engines down.
/// SDK shutdown can report completion without knowing the OS process's outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessExit {
    pub status: ProcessStatus,
    pub at_unix_ns: i64,
}

/// Written by the host when it finalizes its BAML lifetime, read when the
/// recording ends. Internal engine shutdown, such as replacement, leaves it empty.
#[derive(Debug, Default)]
pub struct ProcessExitSlot(std::sync::Mutex<Option<ProcessExit>>);

impl ProcessExitSlot {
    pub fn set(&self, exit: ProcessExit) {
        let mut held = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SDK finalization must not erase an outcome the host already reported.
        if exit.status == ProcessStatus::Unknown
            && held.is_some_and(|exit| exit.status != ProcessStatus::Unknown)
        {
            return;
        }
        *held = Some(exit);
    }

    pub fn get(&self) -> Option<ProcessExit> {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_completion_preserves_a_known_host_outcome() {
        for status in [
            ProcessStatus::Success,
            ProcessStatus::Error,
            ProcessStatus::Panicked,
        ] {
            let slot = ProcessExitSlot::default();
            let known = ProcessExit {
                status,
                at_unix_ns: 1,
            };
            slot.set(known);
            slot.set(ProcessExit {
                status: ProcessStatus::Unknown,
                at_unix_ns: 2,
            });
            assert_eq!(slot.get(), Some(known));
        }
    }
}
