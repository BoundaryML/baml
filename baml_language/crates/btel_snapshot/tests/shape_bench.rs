//! Opt-in release-mode probe of what shaping costs the capturing thread, not
//! a CI assertion. Each workload is captured into a fresh builder (untimed),
//! then shaped (timed) and written to a counting sink (timed) under each
//! policy. Run with:
//!
//! ```sh
//! cargo test --release -p btel_snapshot --test shape_bench -- --ignored --nocapture
//! ```
#![expect(clippy::print_stdout, reason = "machine-readable benchmark results")]
#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "percentile arithmetic over small sample counts"
)]

use std::time::{Duration, Instant};

use baml_type::{DeclarationName, RealizedTy, TypeName, typetag::TypeTag};
use btel_snapshot::{
    BlobScratch, Builder, Limits, ObjectId, ShapePolicy, Shaper, Snapshot, SnapshotObject as O,
    SnapshotPool, SnapshotValue as V,
};

struct Counter(u64);
impl std::io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn text(b: &mut Builder, content: &str) -> V {
    V::String(b.string(&content.into()).unwrap())
}

fn list(b: &mut Builder, items: &[V]) -> V {
    let id = b.reserve_object().unwrap();
    let element_type = b.push_type(RealizedTy::Unknown);
    let start = b.value_start();
    b.reserve_values(items.len());
    for item in items {
        b.push_value(*item);
    }
    let items = b.value_range(start);
    b.set_object(
        id,
        O::List {
            element_type,
            items,
            original_len: items.len(),
        },
    );
    V::Object(id)
}

fn map(b: &mut Builder, entries: &[(String, V)]) -> V {
    let id = b.reserve_object().unwrap();
    let key_type = b.push_type(RealizedTy::String);
    let value_type = b.push_type(RealizedTy::String);
    let start = b.entry_start();
    b.reserve_entries(entries.len());
    for (key, value) in entries {
        b.content(key.len(), false);
        b.entry(&key.as_str().into(), *value);
    }
    let entries = b.entry_range(start);
    b.set_object(
        id,
        O::Map {
            key_type,
            value_type,
            entries,
            original_len: entries.len(),
        },
    );
    V::Object(id)
}

fn declaration(b: &mut Builder) -> ObjectId {
    let id = b.reserve_object().unwrap();
    b.set_object(
        id,
        O::Declaration {
            name: DeclarationName::Declared(TypeName::from_dotted_path("user.Customer")),
            tag: TypeTag::from_i64(42),
            is_enum: false,
        },
    );
    id
}

/// A class instance the size of a typical LLM function input: eight short
/// strings, an int, a bool and a list of three ints.
fn record(b: &mut Builder, declaration: ObjectId, seed: i64) -> V {
    let id = b.reserve_object().unwrap();
    let start = b.type_start();
    let type_arguments = b.type_range(start);
    let mut fields = Vec::with_capacity(11);
    for index in 0..8 {
        fields.push((
            format!("field_{index}"),
            text(b, &format!("value {seed} {index} xx")),
        ));
    }
    fields.push(("count".to_owned(), V::Int(seed)));
    fields.push(("active".to_owned(), V::Bool(true)));
    let scores = list(b, &[V::Int(1), V::Int(2), V::Int(3)]);
    fields.push(("scores".to_owned(), scores));
    let start = b.entry_start();
    b.reserve_entries(fields.len());
    for (key, value) in &fields {
        b.content(key.len(), false);
        b.entry(&key.as_str().into(), *value);
    }
    let fields = b.entry_range(start);
    b.set_object(
        id,
        O::Instance {
            type_arguments,
            declaration,
            fields,
            original_len: fields.len(),
        },
    );
    V::Object(id)
}

type Workload = Box<dyn Fn(&mut Builder) -> V>;

fn workloads() -> Vec<(&'static str, Workload)> {
    let payload = "x".repeat(64 << 10);
    let source = "s".repeat(2 << 10);
    vec![
        ("scalar", Box::new(|_: &mut Builder| V::Int(7))),
        (
            "record",
            Box::new(|b: &mut Builder| {
                let declaration = declaration(b);
                record(b, declaration, 1)
            }),
        ),
        (
            "records-100",
            Box::new(|b: &mut Builder| {
                let declaration = declaration(b);
                let items: Vec<_> = (0..100).map(|seed| record(b, declaration, seed)).collect();
                list(b, &items)
            }),
        ),
        (
            "payload-64k",
            Box::new(move |b: &mut Builder| text(b, &payload)),
        ),
        (
            "images-64x64k",
            Box::new(|b: &mut Builder| {
                let items: Vec<_> = (0..64)
                    .map(|index: u8| {
                        let mut image = vec![b'0'; 64 << 10];
                        image[0] = b'a' + index / 26;
                        image[1] = b'a' + index % 26;
                        text(b, std::str::from_utf8(&image).unwrap())
                    })
                    .collect();
                list(b, &items)
            }),
        ),
        (
            "sources-200x2k",
            Box::new(move |b: &mut Builder| {
                let entries: Vec<_> = (0..200)
                    .map(|index| (format!("src/module_{index}.baml"), text(b, &source)))
                    .collect();
                map(b, &entries)
            }),
        ),
        (
            "cells-10k",
            Box::new(|b: &mut Builder| {
                let mut next = V::Int(0);
                for _ in 0..10_000 {
                    let id = b.reserve_object().unwrap();
                    b.set_object(id, O::Cell(next));
                    next = V::Object(id);
                }
                next
            }),
        ),
    ]
}

fn policies() -> Vec<(&'static str, ShapePolicy)> {
    vec![
        ("whole", ShapePolicy::Whole),
        (
            "split-64k-16k",
            ShapePolicy::Split {
                unit_bytes: 64 << 10,
                leaf_bytes: 16 << 10,
            },
        ),
    ]
}

fn percentile(samples: &mut [Duration], p: f64) -> Duration {
    samples.sort();
    samples[((samples.len() - 1) as f64 * p) as usize]
}

fn micros(d: Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

fn write_all(snapshot: &Snapshot, scratch: &mut BlobScratch) -> u64 {
    let mut counter = Counter(0);
    for blob in snapshot.blobs() {
        blob.write(scratch, &mut counter).unwrap();
    }
    counter.0
}

#[test]
#[ignore = "run explicitly with --release --ignored --nocapture"]
#[expect(
    clippy::assertions_on_constants,
    reason = "reject accidental debug measurements at runtime"
)]
fn shape_bench() {
    assert!(!cfg!(debug_assertions), "measure release builds only");
    let repetitions: usize = std::env::var("SHAPE_BENCH_REPS")
        .map_or(200, |value| value.parse().expect("numeric setting"));
    let pool = SnapshotPool::new(1, Limits::default());
    let mut scratch = BlobScratch::default();
    for (workload, build) in workloads() {
        for (policy_name, policy) in policies() {
            let mut shaper = Shaper::new(policy);
            let mut build_times = Vec::with_capacity(repetitions);
            let mut shape_times = Vec::with_capacity(repetitions);
            let mut write_times = Vec::with_capacity(repetitions);
            let (mut blobs, mut bytes, mut objects) = (0, 0, 0);
            for _ in 0..repetitions {
                let started = Instant::now();
                let mut b = pool.try_acquire().unwrap();
                let root = build(&mut b);
                build_times.push(started.elapsed());
                let started = Instant::now();
                let snapshot = b.finish_value(root, &mut shaper);
                shape_times.push(started.elapsed());
                let started = Instant::now();
                bytes = write_all(&snapshot, &mut scratch);
                write_times.push(started.elapsed());
                blobs = snapshot.blobs().len();
                objects = snapshot.object_count();
            }
            println!(
                "SHAPE_BENCH {workload:<16} {policy_name:<14} objects={objects:<6} blobs={blobs:<4} \
                 bytes={bytes:<9} build_p50_us={:<10.1} shape_p50_us={:<10.1} shape_p99_us={:<10.1} \
                 write_p50_us={:<10.1}",
                micros(percentile(&mut build_times, 0.5)),
                micros(percentile(&mut shape_times, 0.5)),
                micros(percentile(&mut shape_times, 0.99)),
                micros(percentile(&mut write_times, 0.5)),
            );
        }
    }
}
