#![expect(
    clippy::print_stdout,
    reason = "harness-less benchmark emits NDJSON result rows"
)]

use std::{
    collections::BTreeMap,
    hint::black_box,
    time::{Duration, Instant},
};

use bex_events::value_cas::{CanonicalValue, Cid, ValueDag, encode_value_dag};

const SIZE_CURVE: &[usize] = &[
    1024,
    4 * 1024,
    16 * 1024,
    64 * 1024,
    256 * 1024,
    1024 * 1024,
];
const TRANSCRIPT_COUNTS: &[usize] = &[16, 32, 64, 128];
const SAMPLE_WINDOW: Duration = Duration::from_millis(60);
const MIN_SAMPLES: usize = 6;

fn main() {
    emit_size_latency_curve();
    emit_transcript_curve();
}

fn emit_size_latency_curve() {
    for &size_bytes in SIZE_CURVE {
        let value = CanonicalValue::Bytes(deterministic_bytes(size_bytes));
        let median_encode_ns = median_ns(|| {
            black_box(encode_value_dag(black_box(&value)).expect("benchmark value encodes"));
        });
        let dag = encode_value_dag(&value).expect("benchmark value encodes");
        let canonical_bytes = dag
            .chunks
            .iter()
            .map(|chunk| chunk.canonical_bytes.len())
            .sum::<usize>();
        println!(
            "{{\"schema_version\":1,\"bench_id\":\"value_cas_size_{size_bytes}\",\
             \"evidence\":\"measured\",\"input_bytes\":{size_bytes},\
             \"canonical_bytes\":{canonical_bytes},\"chunks\":{},\
             \"median_encode_ns\":{median_encode_ns},\"encode_mb_s\":{:.3}}}",
            dag.chunks.len(),
            throughput_mb_s(size_bytes, median_encode_ns),
        );
    }
}

fn emit_transcript_curve() {
    let prompt = "p".repeat(64 * 1024);
    let max_captures = *TRANSCRIPT_COUNTS.last().expect("non-empty count curve");
    let mut unique_chunks = BTreeMap::<Cid, usize>::new();
    let mut stored_at = BTreeMap::<usize, usize>::new();
    let mut previous_stored_bytes = 0_usize;
    let mut prefix_encode_elapsed_ns = 0_u128;
    for captures in 1..=max_captures {
        let value = transcript(&prompt, captures);
        let prefix_encode_started = Instant::now();
        let dag = encode_value_dag(&value).expect("benchmark prefix encodes");
        prefix_encode_elapsed_ns =
            prefix_encode_elapsed_ns.saturating_add(prefix_encode_started.elapsed().as_nanos());
        for chunk in &dag.chunks {
            unique_chunks
                .entry(chunk.cid)
                .or_insert(chunk.canonical_bytes.len());
        }
        let stored_bytes = unique_chunks.values().sum::<usize>();
        let incremental_bytes = stored_bytes.saturating_sub(previous_stored_bytes);
        previous_stored_bytes = stored_bytes;
        if !TRANSCRIPT_COUNTS.contains(&captures) {
            continue;
        }
        stored_at.insert(captures, stored_bytes);

        let median_encode_ns = median_ns(|| {
            black_box(encode_value_dag(black_box(&value)).expect("benchmark value encodes"));
        });
        let (hash_median_ns, hash_input_bytes) = hash_cpu(&dag);
        let legacy_body_bytes = prompt
            .len()
            .saturating_mul(captures)
            .saturating_mul(captures.saturating_add(1))
            / 2;
        let reduction = legacy_body_bytes as f64 / stored_bytes.max(1) as f64;
        let growth_exponent = if captures > 16 {
            (stored_bytes as f64 / stored_at[&16] as f64).ln() / (captures as f64 / 16.0).ln()
        } else {
            1.0
        };
        let throughput = throughput_mb_s(
            legacy_body_bytes,
            u64::try_from(prefix_encode_elapsed_ns).unwrap_or(u64::MAX),
        );
        println!(
            "{{\"schema_version\":1,\"bench_id\":\"value_cas_transcript_{captures}\",\
             \"evidence\":\"measured\",\"prompt_bytes\":{},\"captures\":{captures},\
             \"median_encode_ns\":{median_encode_ns},\"final_chunks\":{},\
             \"legacy_body_bytes\":{legacy_body_bytes},\"cas_unique_bytes\":{stored_bytes},\
             \"reduction_x\":{reduction:.3},\"incremental_bytes\":{incremental_bytes},\
             \"growth_exponent_16_to_64\":{growth_exponent:.3},\
             \"prefix_encode_elapsed_ns\":{prefix_encode_elapsed_ns},\
             \"logical_input_bytes\":{legacy_body_bytes},\"throughput_mb_s\":{throughput:.3},\
             \"hash_input_bytes\":{hash_input_bytes},\"hash_median_ns\":{hash_median_ns},\
             \"hash_mb_s\":{:.3}}}",
            prompt.len(),
            dag.chunks.len(),
            throughput_mb_s(hash_input_bytes, hash_median_ns),
        );
    }
    println!(
        "{{\"schema_version\":1,\"bench_id\":\"value_cas_transcript_prefix_cpu\",\
         \"evidence\":\"measured\",\"prompt_bytes\":{},\"captures\":{max_captures},\
         \"prefix_encode_elapsed_ns\":{prefix_encode_elapsed_ns},\"logical_input_bytes\":{},\
         \"throughput_mb_s\":{:.3}}}",
        prompt.len(),
        prompt
            .len()
            .saturating_mul(max_captures)
            .saturating_mul(max_captures.saturating_add(1))
            / 2,
        throughput_mb_s(
            prompt
                .len()
                .saturating_mul(max_captures)
                .saturating_mul(max_captures.saturating_add(1))
                / 2,
            u64::try_from(prefix_encode_elapsed_ns).unwrap_or(u64::MAX),
        ),
    );
}

fn hash_cpu(dag: &ValueDag) -> (u64, usize) {
    let input_bytes = dag
        .chunks
        .iter()
        .map(|chunk| chunk.canonical_bytes.len())
        .sum::<usize>();
    let elapsed = median_ns(|| {
        for chunk in &dag.chunks {
            black_box(Cid::for_node(black_box(&chunk.canonical_bytes)));
        }
    });
    (elapsed, input_bytes)
}

fn median_ns(mut measured: impl FnMut()) -> u64 {
    let deadline = Instant::now() + SAMPLE_WINDOW;
    let mut samples = Vec::new();
    while Instant::now() < deadline || samples.len() < MIN_SAMPLES {
        let started = Instant::now();
        measured();
        samples.push(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
    }
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn throughput_mb_s(bytes: usize, elapsed_ns: u64) -> f64 {
    if elapsed_ns == 0 {
        return 0.0;
    }
    bytes as f64 / (elapsed_ns as f64 / 1e9) / 1_000_000.0
}

fn deterministic_bytes(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| ((index.wrapping_mul(31).wrapping_add(index / 251)) & 0xff) as u8)
        .collect()
}

fn transcript(prompt: &str, messages: usize) -> CanonicalValue {
    CanonicalValue::List(
        (0..messages)
            .map(|index| {
                CanonicalValue::Map(vec![
                    (
                        "content".to_string(),
                        CanonicalValue::String(prompt.to_owned()),
                    ),
                    (
                        "sequence".to_string(),
                        CanonicalValue::Int(i64::try_from(index).unwrap_or(i64::MAX)),
                    ),
                ])
            })
            .collect(),
    )
}
