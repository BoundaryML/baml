//! `.bamlprof` file writing and reading (v2 §4 framing).
//!
//! Framing: one length-delimited [`pb::EventFileHeaderV1`], then a stream of
//! length-delimited [`pb::DiskEventV1`] messages. One file per engine per
//! process: `<process_id>-<started_at>-<engine>.bamlprof`.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub use crate::prof::read::{
    BamlprofContents, header_started_at_epoch_ns, read_bamlprof_from_bytes,
    read_bamlprof_from_reader,
};
use crate::prof::{
    artifact::{ProfileArtifactRef, ProfileArtifactSink},
    encode::{encode_disk_event, encode_length_delimited_message},
    pb,
};

/// A single engine's open profile file.
pub(crate) struct ProfileWriter {
    file: File,
    path: PathBuf,
    /// Reused length-delimited encode buffer. One allocation that grows to
    /// the largest message, then stays — avoids a `Vec` allocation (and the
    /// extra `encoded_len` walk a pre-sized `with_capacity` would cost) on
    /// every event written. The consumer transcodes one event per record on
    /// its hot path, so this is the difference between zero and one malloc
    /// per traced event.
    scratch: Vec<u8>,
}

impl ProfileWriter {
    /// Creates the profiles directory if needed, the file, and writes the
    /// header.
    pub(crate) fn create(
        dir: &Path,
        process_id: [u8; 16],
        started_at_epoch_ns: u128,
        engine_id: u64,
        header: &pb::EventFileHeaderV1,
    ) -> io::Result<ProfileWriter> {
        fs::create_dir_all(dir)?;
        let started_at_secs = started_at_epoch_ns / 1_000_000_000;
        let name = format!(
            "{}-{started_at_secs}-{engine_id}.bamlprof",
            hex_uuid(process_id)
        );
        let path = dir.join(name);
        let file = File::create(&path)?;
        let mut writer = ProfileWriter {
            file,
            path,
            scratch: Vec::new(),
        };
        writer.write_message(header)?;
        Ok(writer)
    }

    /// Appends one length-delimited event to the in-flight range buffer.
    /// Native files and WASM byte/chunk sinks share the target-neutral encoder.
    pub(crate) fn encode_event(&mut self, event: &pb::DiskEventV1) {
        encode_disk_event(&mut self.scratch, event);
    }

    /// Write the accumulated range buffer to the file in one syscall and reset
    /// it. No-op when nothing is buffered.
    pub(crate) fn flush_buffered(&mut self) -> io::Result<()> {
        if !self.scratch.is_empty() {
            self.file.write_all(&self.scratch)?;
            self.scratch.clear();
        }
        Ok(())
    }

    fn write_message(&mut self, msg: &impl prost::Message) -> io::Result<()> {
        // One-shot header write at file creation: encode + a single write.
        self.scratch.clear();
        encode_length_delimited_message(&mut self.scratch, msg).map_err(io::Error::other)?;
        self.file.write_all(&self.scratch)?;
        self.scratch.clear();
        Ok(())
    }

    /// Flush buffered range bytes to the OS (no `fsync`). The cadence flush.
    pub(crate) fn flush(&mut self) -> io::Result<()> {
        self.flush_buffered()
    }

    /// Flush + `fsync`: survives power loss, not just process exit. Used for
    /// explicit flush requests and engine closes.
    pub(crate) fn sync(&mut self) -> io::Result<()> {
        self.flush_buffered()?;
        self.file.sync_all()
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl ProfileArtifactSink for ProfileWriter {
    fn write_chunk(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.flush_buffered()?;
        self.file.write_all(bytes)
    }

    fn flush(&mut self) -> io::Result<ProfileArtifactRef> {
        self.flush_buffered()?;
        Ok(ProfileArtifactRef::NativeFile {
            path: self.path.clone(),
        })
    }
}

fn hex_uuid(bytes: [u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Reads a `.bamlprof` back: the header and every whole event, tolerating a
/// torn trailing message. Errors only when the file or its header is
/// unreadable. The reader for tests, gates, and ad-hoc tooling - the M5
/// renderer supersedes it for real consumption.
pub fn read_bamlprof(path: &Path) -> io::Result<BamlprofContents> {
    let bytes = fs::read(path)?;
    read_bamlprof_from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::ProfileWriter;
    use crate::prof::{
        artifact::{ByteProfileArtifactSink, ProfileArtifactRef, ProfileArtifactSink},
        encode::{encode_disk_event, encode_length_delimited_message},
        pb,
    };

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!(
            "bamlprof-file-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn fixed_header() -> pb::EventFileHeaderV1 {
        pb::EventFileHeaderV1 {
            process_id: vec![1; 16],
            engine_id: 7,
            program_id: "program".to_string(),
            started_at_epoch_ns: 123u128.to_le_bytes().to_vec(),
            function_table: None,
            clock_kind: pb::ClockKind::Instant as i32,
            tick_ns_numer: 1,
            tick_ns_denom: 1,
            clock_quality: pb::ClockQuality::Exact as i32,
            source_snapshot_id: None,
            revision_id: None,
        }
    }

    fn fixed_events() -> Vec<pb::DiskEventV1> {
        vec![
            pb::DiskEventV1 {
                event: Some(pb::disk_event_v1::Event::CallFunction(pb::CallFunction {
                    thread_id: 1,
                    call_id: 2,
                    parent_call_id: None,
                    function_id: 3,
                    timestamp_ns: 4,
                    call_site_file_id: None,
                    call_site_start_offset: None,
                    call_site_end_offset: None,
                    call_site_line: None,
                })),
            },
            pb::DiskEventV1 {
                event: Some(pb::disk_event_v1::Event::EndFunction(pb::EndFunction {
                    thread_id: 1,
                    call_id: 2,
                    status: pb::FunctionEndStatus::Ok as i32,
                    timestamp_ns: 9,
                })),
            },
        ]
    }

    #[test]
    fn native_file_writer_and_byte_sink_share_byte_contract() {
        let dir = temp_dir("byte-contract");
        let process_id = [0xAB; 16];
        let started_at_epoch_ns = 123_456_789_000u128;
        let engine_id = 7;
        let header = fixed_header();
        let events = fixed_events();

        let mut writer =
            ProfileWriter::create(&dir, process_id, started_at_epoch_ns, engine_id, &header)
                .unwrap();
        for event in &events {
            writer.encode_event(event);
        }
        writer.sync().unwrap();
        let native_path = writer.path().to_path_buf();
        drop(writer);
        let native_bytes = std::fs::read(&native_path).unwrap();

        let mut sink = ByteProfileArtifactSink::new();
        let mut shared_bytes = Vec::new();
        encode_length_delimited_message(&mut shared_bytes, &header).unwrap();
        for event in &events {
            encode_disk_event(&mut shared_bytes, event);
        }
        sink.write_chunk(&shared_bytes).unwrap();
        assert_eq!(
            sink.flush().unwrap(),
            ProfileArtifactRef::Bytes {
                len: native_bytes.len(),
                truncated: false,
                dropped_bytes: 0,
                dropped_chunks: 0
            }
        );

        assert_eq!(native_bytes, sink.bytes());
        let parsed = super::read_bamlprof(&native_path).unwrap();
        assert_eq!(parsed.events, events);
        let _ = std::fs::remove_file(native_path);
        let _ = std::fs::remove_dir(dir);
    }
}
