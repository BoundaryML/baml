//! Snapshots the values of a BCS benchmark corpus with this checkout's writer, since BCS builds
//! no BAML crate:
//!
//! ```sh
//! (cd <baml>/baml_language && cargo run --release -p btel_bcs --example bcs_bench_snapshots) \
//!   < captures.jsonl > snapshots.bin
//! ```
//!
//! It reads a corpus: a line of JSON that names it, then one value on each line,
//! `{"arguments": [text, ...]}`, `{"text": text}` or `{"map": [[key, text], ...]}`. It writes a
//! line of JSON with the blob version this checkout's writer writes, its workspace version and
//! channel, the sealing limits of its default recorder and cloud publisher, and the corpus's
//! first line; then for each value, in order, the count of its blobs as a little-endian `u32`
//! and each blob as its length, a little-endian `u32`, and its bytes, children first and the
//! root last.

use std::io::{self, BufRead as _, Write as _};

use btel_bcs::CloudPublisherConfig;
use btel_recorder::RecordingConfig;
use btel_snapshot::{BexStr, BlobScratch, Limits, Shaper, Snapshot, SnapshotPool};
use serde_json::{Value as Json, json};

/// One value of the corpus, captured as the runtime captures it: shaped by its default policy.
fn capture(pool: &SnapshotPool, value: &Json) -> io::Result<Snapshot> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("not a capture: {value}"),
        )
    };
    let text = |value: &Json| value.as_str().map(BexStr::from).ok_or_else(invalid);
    let mut builder = pool.try_acquire().ok_or_else(invalid)?;
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`arguments` needs a closure over every `Leaves` lifetime; the method path names one"
    )]
    let root = if let Some(arguments) = value.get("arguments").and_then(Json::as_array) {
        let texts = arguments.iter().map(text).collect::<io::Result<Vec<_>>>()?;
        builder.arguments(texts.iter(), |leaves, text| leaves.string_value(text))
    } else if let Some(body) = value.get("text") {
        builder.leaves().string_value(&text(body)?).into()
    } else if let Some(entries) = value.get("map").and_then(Json::as_array) {
        let entries = entries
            .iter()
            .map(|entry| match entry.as_array().map(Vec::as_slice) {
                Some([key, value]) => Ok((
                    key.as_str().ok_or_else(invalid)?.to_owned(),
                    value.as_str().ok_or_else(invalid)?.to_owned(),
                )),
                _ => Err(invalid()),
            })
            .collect::<io::Result<Vec<_>>>()?;
        drop(builder);
        return btel_snapshot::string_map(pool, &entries, usize::MAX).ok_or_else(invalid);
    } else {
        return Err(invalid());
    };
    Ok(builder.finish(root, &mut Shaper::default()))
}

fn main() -> io::Result<()> {
    let mut lines = io::stdin().lock().lines();
    let first = lines.next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::UnexpectedEof, "no line names the corpus")
    })??;
    let captures: Json = serde_json::from_str(&first)?;
    let recorder = RecordingConfig::default();
    let publisher = CloudPublisherConfig::default();
    let written = json!({
        "blobVersion": btel_core::cas::VERSION,
        "sourceVersion": baml_version::CANONICAL_VERSION,
        "channel": baml_version::CHANNEL,
        "sealing": {
            "flushIntervalNs": u64::try_from(recorder.flush_interval_duration.as_nanos())
                .expect("a flush interval within u64 nanoseconds"),
            "targetBytes": recorder.target_bytes.get(),
            "snapshotTarget": publisher.candidate_target,
            "snapshotBytesTarget": publisher.retained_bytes_target,
            "maxOffered": publisher.max_offered_ids,
        },
        "captures": captures,
    });
    let mut out = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(&mut out, &written)?;
    out.write_all(b"\n")?;
    let pool = SnapshotPool::unbounded(Limits::default());
    let mut scratch = BlobScratch::default();
    let mut blob = Vec::new();
    for line in lines {
        let snapshot = capture(&pool, &serde_json::from_str(&line?)?)?;
        let count = u32::try_from(snapshot.blobs().len()).expect("a capture's blobs fit a u32");
        out.write_all(&count.to_le_bytes())?;
        for each in snapshot.blobs() {
            blob.clear();
            each.write(&mut scratch, &mut blob)?;
            let length = u32::try_from(blob.len()).expect("a blob fits a u32 length");
            out.write_all(&length.to_le_bytes())?;
            out.write_all(&blob)?;
        }
    }
    out.flush()
}
