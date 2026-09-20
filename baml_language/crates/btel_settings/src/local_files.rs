//! Local delivery settings. Destination paths are supplied explicitly per engine.
use std::num::NonZeroUsize;

/// **Tune first for disk bursts.** Queued sealed files, excluding the one being
/// written. More slots absorb short disk stalls but retain more encoded memory.
/// All queued/in-flight files still count against the publisher's byte budget.
/// Exhaustion fails explicitly; it never blocks the processor on filesystem I/O.
pub const QUEUE_FILES: NonZeroUsize = NonZeroUsize::new(8).unwrap();

#[derive(Clone, Copy, Debug)]
pub struct FileSinkConfig {
    pub queue_files: NonZeroUsize,
}
impl Default for FileSinkConfig {
    fn default() -> Self {
        Self {
            queue_files: QUEUE_FILES,
        }
    }
}
