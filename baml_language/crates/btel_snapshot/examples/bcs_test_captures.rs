//! Prints the captures BCS's tests read: values as this checkout's writer captures them, for
//! tests that need real blobs and build none, since BCS builds no BAML crate.
//!
//! ```sh
//! cargo run -p btel_snapshot --example bcs_test_captures > captures.hex
//! ```
//!
//! BCS keeps one such file for each blob version, named for it (`blob-v6.hex` for version 6);
//! the file's first line names the blob version it holds and the workspace version and channel
//! of the checkout that wrote it. A capture is a `== <name>` line and then its blobs in hex, one
//! on each line, children first and the root last. BCS's tests ask for a capture by its name,
//! so a name says what the value is, and a capture is added here first.

use std::io::{self, Write as _};

use btel_snapshot::{
    BexStr, BlobScratch, Limits, OwnedType, Shaper, Snapshot, SnapshotPool, SnapshotValue,
};

/// A context as the runtime captures one: a map of its `distinct_id`, a string or null, and its
/// `metadata`, a map of strings.
fn context(distinct_id: Option<&str>, metadata: &[(&str, &str)]) -> Snapshot {
    let pool = SnapshotPool::unbounded(Limits::default());
    let mut builder = pool.try_acquire().unwrap();
    let key_type = builder.leaves().ty(OwnedType::string());
    let value_type = builder.leaves().ty(OwnedType::unknown());
    let entries = builder.map(
        key_type,
        value_type,
        metadata.iter(),
        |leaves, (key, value)| {
            (
                leaves.string_value(&BexStr::from(*key)),
                leaves.string_value(&BexStr::from(*value)),
            )
        },
    );
    let entries = builder.leaves().object(entries).unwrap();
    let distinct_id = match distinct_id {
        Some(text) => builder.leaves().string_value(&BexStr::from(text)),
        None => SnapshotValue::Null,
    };
    let fields = [
        ("distinct_id", distinct_id),
        ("metadata", SnapshotValue::Object(entries)),
    ];
    let key_type = builder.leaves().ty(OwnedType::string());
    let value_type = builder.leaves().ty(OwnedType::unknown());
    let root = builder.map(
        key_type,
        value_type,
        fields.into_iter(),
        |leaves, (key, value)| (leaves.string_value(&BexStr::from(key)), value),
    );
    let root = builder.leaves().object(root).unwrap();
    builder.finish(SnapshotValue::Object(root), &mut Shaper::default())
}

/// A string value.
fn text(text: &str) -> Snapshot {
    let pool = SnapshotPool::unbounded(Limits::default());
    let mut builder = pool.try_acquire().unwrap();
    let root = builder.leaves().string_value(&BexStr::from(text));
    builder.finish(root, &mut Shaper::default())
}

/// A map of one string.
fn string_map(key: &str, value: &str) -> Snapshot {
    let pool = SnapshotPool::unbounded(Limits::default());
    btel_snapshot::string_map(&pool, &[(key.to_owned(), value.to_owned())], usize::MAX).unwrap()
}

/// A function's two arguments: an int and a string.
fn arguments(value: i64, label: &str) -> Snapshot {
    let pool = SnapshotPool::unbounded(Limits::default());
    let mut builder = pool.try_acquire().unwrap();
    let root = builder.arguments(0..2, |leaves, slot| match slot {
        0 => SnapshotValue::Int(value),
        _ => leaves.string_value(&BexStr::from(label)),
    });
    builder.finish(root, &mut Shaper::default())
}

fn int(value: i64) -> Snapshot {
    let pool = SnapshotPool::unbounded(Limits::default());
    pool.try_acquire()
        .unwrap()
        .finish(SnapshotValue::Int(value), &mut Shaper::default())
}

/// A type whose description is longer than a reader takes: lists of a string, `levels` deep.
fn deep_type(levels: usize) -> Snapshot {
    let pool = SnapshotPool::unbounded(Limits::default());
    let mut builder = pool.try_acquire().unwrap();
    let mut ty = OwnedType::String;
    for _ in 0..levels {
        ty = OwnedType::List(Box::new(ty));
    }
    let id = builder.leaves().ty(ty);
    builder.finish(SnapshotValue::Type(id), &mut Shaper::default())
}

/// An HTTP response's connection: its status and its headers.
fn connection(status: i64, headers: &[(&str, &str)]) -> Snapshot {
    let pool = SnapshotPool::unbounded(Limits::default());
    let mut builder = pool.try_acquire().unwrap();
    let key_type = builder.leaves().ty(OwnedType::string());
    let value_type = builder.leaves().ty(OwnedType::unknown());
    let values: Vec<_> = headers
        .iter()
        .map(|(key, value)| (*key, builder.leaves().string_value(&BexStr::from(*value))))
        .collect();
    let headers = builder.map(
        key_type,
        value_type,
        values.into_iter(),
        |leaves, (key, value)| (leaves.string_value(&BexStr::from(key)), value),
    );
    let headers = builder.leaves().object(headers).unwrap();
    let root = builder.map(
        key_type,
        value_type,
        [
            ("status", SnapshotValue::Int(status)),
            ("headers", SnapshotValue::Object(headers)),
        ]
        .into_iter(),
        |leaves, (key, value)| (leaves.string_value(&BexStr::from(key)), value),
    );
    let root = builder.leaves().object(root).unwrap();
    builder.finish(SnapshotValue::Object(root), &mut Shaper::default())
}

/// Every capture, by the name BCS's tests ask for it by.
fn captures() -> Vec<(String, Snapshot)> {
    let mut captures: Vec<(String, Snapshot)> = Vec::new();
    let mut add = |name: String, snapshot: Snapshot| captures.push((name, snapshot));
    for body in [
        "inputs",
        r#"{"metadata":{"tier":"gold"},"distinct_id":"u-1"}"#,
        r#"{"metadata":{"tier":"gold"}}"#,
        r#"{"choices":[{"delta":"1"}]}"#,
        r#"{"choices":[{"delta":"2"}]}"#,
        r#"{"model":"gpt-6-sol","usage":{"prompt_tokens":100,"completion_tokens":10}}"#,
        "out of stock",
    ] {
        add(format!("text {body}"), text(body));
    }
    add(
        "context user-1 commit=abc123".into(),
        context(Some("user-1"), &[("commit", "abc123")]),
    );
    add("context anonymous".into(), context(None, &[]));
    // A distinct ID long enough to be a blob of its own.
    for letter in ['u', 'v'] {
        let distinct_id = letter.to_string().repeat(20_000);
        add(
            format!("context 20000{letter}"),
            context(Some(&distinct_id), &[]),
        );
    }
    add("map label=x".into(), string_map("label", "x"));
    add("arguments 7 first".into(), arguments(7, "first"));
    add("arguments 3 second".into(), arguments(3, "second"));
    add("int 7".into(), int(7));
    // One past the 4,096 bytes of description a reader takes.
    add("type past the reader's limit".into(), deep_type(4_097));
    add("map url=u".into(), string_map("url", "u"));
    add(
        "map status=500 headers={x-request-id=preserved}".into(),
        connection(500, &[("x-request-id", "preserved")]),
    );
    for body in [
        "transport timed out: private request context",
        "not a context",
        "a chunk",
    ] {
        add(format!("text {body}"), text(body));
    }
    for (distinct_id, tier) in [("user-1", "gold"), ("user-2", "free"), ("tenant-7", "gold")] {
        add(
            format!("context {distinct_id} tier={tier}"),
            context(Some(distinct_id), &[("tier", tier)]),
        );
    }
    // One context per request of a server of 100 files of 8 requests each.
    let (files, each) = (100_u64, 8_u64);
    for sequence in 1..=files + 1 {
        for request in 0..each.min((files + 1 - sequence) * each) {
            let user = format!("user-{sequence}-{request}");
            add(
                format!("context {user} tier=gold"),
                context(Some(&user), &[("tier", "gold")]),
            );
        }
    }
    captures
}

fn main() -> io::Result<()> {
    // Building, writing and dropping a type thousands of levels deep recurses as deep: do it all
    // where a debug build has the stack for it.
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(print)?
        .join()
        .expect("printing the captures")
}

fn print() -> io::Result<()> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    writeln!(
        out,
        "# Captures of blob version {}, written by a BAML checkout's own writer (workspace version {}, channel {}).",
        btel_core::cas::VERSION,
        baml_version::CANONICAL_VERSION,
        baml_version::CHANNEL,
    )?;
    writeln!(
        out,
        "# Generated by BAML's `cargo run -p btel_snapshot --example bcs_test_captures`; never edited by hand."
    )?;
    writeln!(
        out,
        "# A capture is its name and then its blobs, children first and the root last."
    )?;
    let mut scratch = BlobScratch::default();
    for (name, snapshot) in captures() {
        writeln!(out, "== {name}")?;
        for blob in snapshot.blobs() {
            let mut bytes = Vec::new();
            blob.write(&mut scratch, &mut bytes)?;
            for byte in &bytes {
                write!(out, "{byte:02x}")?;
            }
            writeln!(out)?;
        }
    }
    out.flush()
}
