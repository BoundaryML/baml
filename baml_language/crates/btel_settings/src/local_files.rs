//! Local delivery settings. File output is enabled explicitly per engine.
use std::num::NonZeroUsize;

/// Standard recording directory relative to the CLI-resolved BAML project root.
/// Packed binaries without a project root use the current user's home directory.
/// Each recording gets its own ID directory beneath this path.
pub const RECORDINGS_DIRECTORY: &str = ".baml/btel/recordings";

/// **Tune first for disk bursts.** Queued sealed files, excluding the one being
/// written. More slots absorb short disk stalls but retain more encoded memory.
/// Exhaustion blocks downstream delivery after input chunks have been released.
/// Increasing this limit absorbs bursts; it cannot fix sustained writer lag.
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
