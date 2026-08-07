use std::io::Write;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use baml_query::{LocalBlobStore, QueryEngine, SqliteFunctionCallsProvider, ValueId};
use divan::{Bencher, black_box};
use rusqlite::Connection;
use serde_json::json;
use tempfile::TempDir;
use tokio::runtime::Runtime;

struct Fixture {
    _temp_dir: TempDir,
    engine: QueryEngine,
}

impl Fixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("create benchmark temp directory");
        let store = LocalBlobStore::new(temp_dir.path());
        let hit_bytes = serde_json::to_vec(&json!(["needle"])).expect("encode hit value");
        let miss_bytes = serde_json::to_vec(&json!(["other"])).expect("encode miss value");
        let hit_id = ValueId::from_content(&hit_bytes);
        let miss_id = ValueId::from_content(&miss_bytes);
        std::fs::create_dir_all(temp_dir.path().join("values")).expect("create values directory");
        std::fs::write(store.path_for(hit_id), hit_bytes).expect("write hit value");
        std::fs::write(store.path_for(miss_id), miss_bytes).expect("write miss value");

        let connection = Connection::open_in_memory().expect("open SQLite");
        connection
            .execute_batch(
                "CREATE TABLE function_calls (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    process_id TEXT,
                    thread_id TEXT,
                    cct_id TEXT,
                    captured_ts INTEGER,
                    name TEXT NOT NULL,
                    status TEXT,
                    metadata TEXT,
                    metrics TEXT,
                    args_value_id BLOB,
                    return_value_id BLOB,
                    error_value_id BLOB
                );",
            )
            .expect("create benchmark table");
        let transaction = connection
            .unchecked_transaction()
            .expect("start benchmark transaction");
        for index in 0..10_000 {
            let value_id = if index < 9_000 { miss_id } else { hit_id };
            transaction
                .execute(
                    "INSERT INTO function_calls
                     (id, project_id, name, args_value_id, captured_ts)
                     VALUES (?1, 'benchmark-project', 'send_email', ?2, ?3)",
                    rusqlite::params![
                        format!("call-{index}"),
                        value_id.as_bytes().to_vec(),
                        i64::from(index),
                    ],
                )
                .expect("insert benchmark row");
        }
        transaction.commit().expect("commit benchmark rows");

        let provider = SqliteFunctionCallsProvider::from_connection(
            connection,
            temp_dir.path(),
            Arc::<str>::from("benchmark-project"),
        )
        .expect("create provider");
        Self {
            _temp_dir: temp_dir,
            engine: QueryEngine::new(provider).expect("create query engine"),
        }
    }
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(Fixture::new)
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("create benchmark runtime"))
}

fn main() {
    if cfg!(debug_assertions) {
        return;
    }
    if std::env::var_os("BAML_QUERY_MANUAL_BENCH").is_some() {
        manual_benchmark();
        return;
    }
    divan::main();
}

fn manual_benchmark() {
    let fixture = fixture();
    let runtime = runtime();
    benchmark_query(
        fixture,
        runtime,
        "resident_filter",
        "SELECT id FROM function_calls WHERE name = 'send_email'",
    );
    benchmark_query(
        fixture,
        runtime,
        "hydrated_filter_with_final_limit",
        "SELECT id FROM function_calls
         WHERE contains(value_at(args, 0), 'needle')
         LIMIT 100",
    );
    benchmark_query(
        fixture,
        runtime,
        "hydrated_projection",
        "SELECT args FROM function_calls WHERE id = 'call-9999'",
    );
    benchmark_query(
        fixture,
        runtime,
        "resident_group_by",
        "SELECT name, COUNT(*) FROM function_calls GROUP BY name",
    );
    benchmark_query(
        fixture,
        runtime,
        "hydrated_group_by",
        "SELECT value_string(value_at(args, 0)), COUNT(*)
         FROM function_calls
         GROUP BY value_string(value_at(args, 0))",
    );
}

fn benchmark_query(fixture: &Fixture, runtime: &Runtime, name: &str, sql: &str) {
    let mut samples = Vec::with_capacity(5);
    for _ in 0..5 {
        let started = Instant::now();
        let batches = runtime
            .block_on(fixture.engine.execute(sql))
            .expect("benchmark query succeeds");
        black_box(batches);
        samples.push(started.elapsed().as_nanos());
    }
    let min = samples.iter().copied().min().unwrap_or_default();
    let max = samples.iter().copied().max().unwrap_or_default();
    let mean = samples.iter().sum::<u128>() / samples.len() as u128;
    let metrics = fixture.engine.metrics();
    write_line(&format!(
        "{name}: min={}us mean={}us max={}us candidates={} output={} sqlite={}us hydration={}us blobs={} blob_bytes={} serialization={}us cache_hits={} cache_misses={}",
        min / 1_000,
        mean / 1_000,
        max / 1_000,
        metrics.input_rows,
        metrics.output_rows,
        metrics.sqlite_duration.as_nanos() / 1_000,
        metrics.hydration_duration.as_nanos() / 1_000,
        metrics.blob_requests,
        metrics.blob_bytes,
        metrics.serialization_duration.as_nanos() / 1_000,
        metrics.cache_hits,
        metrics.cache_misses,
    ));
}

fn write_line(line: &str) {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let _ = writeln!(stdout, "{line}");
}

#[divan::bench]
fn resident_filter(bencher: Bencher) {
    let fixture = fixture();
    let runtime = runtime();
    bencher.bench(|| {
        let batches = runtime.block_on(fixture.engine.execute(black_box(
            "SELECT id FROM function_calls WHERE name = 'send_email'",
        )));
        black_box(batches)
    });
}

#[divan::bench]
fn hydrated_filter_with_final_limit(bencher: Bencher) {
    let fixture = fixture();
    let runtime = runtime();
    bencher.bench(|| {
        let batches = runtime.block_on(fixture.engine.execute(black_box(
            "SELECT id FROM function_calls
                 WHERE contains(value_at(args, 0), 'needle')
                 LIMIT 100",
        )));
        black_box(batches)
    });
}

#[divan::bench]
fn hydrated_projection(bencher: Bencher) {
    let fixture = fixture();
    let runtime = runtime();
    bencher.bench(|| {
        let batches = runtime.block_on(fixture.engine.execute(black_box(
            "SELECT args FROM function_calls WHERE id = 'call-9999'",
        )));
        black_box(batches)
    });
}

#[divan::bench]
fn resident_group_by(bencher: Bencher) {
    let fixture = fixture();
    let runtime = runtime();
    bencher.bench(|| {
        let batches = runtime.block_on(fixture.engine.execute(black_box(
            "SELECT name, COUNT(*) FROM function_calls GROUP BY name",
        )));
        black_box(batches)
    });
}

#[divan::bench]
fn hydrated_group_by(bencher: Bencher) {
    let fixture = fixture();
    let runtime = runtime();
    bencher.bench(|| {
        let batches = runtime.block_on(fixture.engine.execute(black_box(
            "SELECT value_string(value_at(args, 0)), COUNT(*)
             FROM function_calls
             GROUP BY value_string(value_at(args, 0))",
        )));
        black_box(batches)
    });
}
