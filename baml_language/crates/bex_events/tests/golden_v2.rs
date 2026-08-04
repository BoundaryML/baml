//! Golden-fixture verifier for `testdata/golden/v2/` (design §6.9): the P3
//! on-disk observability formats — BCCT session segments (`.bamlseg`),
//! BMET meta streams (`.bamlmeta`), sealed boundary snapshots
//! (`.bamlcct`), and the raw firehose container (`raw-*.bamlprof`).
//!
//! Each fixture has a deterministic builder. Normal runs assert the
//! builder's bytes equal the committed file byte-for-byte (encoder drift
//! fails CI) and that readers honor the §6.3 committed-prefix recovery
//! contract on torn tails. `BAML_GOLDEN_WRITE=1` regenerates — legitimate
//! exactly once per new fixture; changing an existing file means "bump the
//! format version and mint new fixtures", not "regenerate".

use bex_events::ids::{BexCallId, BexThreadId, FunctionId};
use bex_events::prof::cct::CctEngine;
use bex_events::prof::cct::blocks;
use bex_events::prof::cct::fold;
use bex_events::prof::cct::meta::{self, MetaRecord};
use bex_events::prof::cct::raw::{self, RawSink};
use bex_events::prof::cct::segment::{self, BlockKind, ScanEnd, SegmentHeader};
use bex_events::prof::cct::session::{FsyncService, SessionWriter};
use bex_events::prof::record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus};

fn golden_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/golden/v2")
}

/// Byte-compare (or regenerate under `BAML_GOLDEN_WRITE=1`).
fn assert_golden(name: &str, built: &[u8]) {
    let path = golden_dir().join(name);
    if std::env::var("BAML_GOLDEN_WRITE").as_deref() == Ok("1") {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, built).unwrap();
        return;
    }
    let committed = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "missing golden fixture {} ({err}); generate once with \
             BAML_GOLDEN_WRITE=1 cargo test -p bex_events --test golden_v2",
            path.display()
        )
    });
    assert!(
        committed == built,
        "{name}: builder output differs from the FROZEN v2 fixture \
         ({} committed vs {} built bytes). Sealed v2 bytes never change — \
         if an encoder legitimately changed, bump the container version \
         and mint new fixtures.",
        committed.len(),
        built.len()
    );
}

/// Encode a record sequence into one drained-range byte buffer.
fn encode_records(records: &[RawRecord<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; MAX_RECORD_LEN];
    for rec in records {
        let len = rec.encode(&mut buf);
        out.extend_from_slice(&buf[..len]);
    }
    out
}

/// The deterministic mini-program every fixture is built from: one root
/// thread, two functions (one erroring call), fixed tick timestamps under
/// an identity clock.
fn fixture_records() -> Vec<u8> {
    encode_records(&[
        RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 1_000,
            name: b"",
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            parent_call_id: BexCallId(0),
            function_id: FunctionId(16),
            call_site: None,
            ts_ticks: 2_000,
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
            parent_call_id: BexCallId(1),
            function_id: FunctionId(17),
            call_site: None,
            ts_ticks: 3_000,
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Errored,
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
            ts_ticks: 4_000,
        },
        RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(3),
            parent_call_id: BexCallId(1),
            function_id: FunctionId(17),
            call_site: None,
            ts_ticks: 5_000,
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(3),
            ts_ticks: 6_000,
        },
        RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            ts_ticks: 7_000,
        },
        RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: BexThreadId(1),
            ts_ticks: 8_000,
        },
    ])
}

fn fixture_engine() -> CctEngine {
    let mut engine = CctEngine::new(32);
    engine.consume(&fixture_records(), &mut |t| t);
    engine
}

const EUID: [u8; 16] = [0xA5; 16];
const ENGINE_ID: u64 = 7;
const STARTED_NS: u64 = 1_700_000_000_000_000_000;
const CLOCK: (u8, u8, u64, u64) = (1, 2, 1, 1);
const REVISION: [u8; 32] = [0x42; 32];

fn fixture_header() -> SegmentHeader {
    SegmentHeader {
        process_euid: EUID,
        engine_id: ENGINE_ID,
        session_seg_seq: 0,
        started_epoch_ns: STARTED_NS,
        clock_kind: CLOCK.0,
        clock_quality: CLOCK.1,
        tick_ns_numer: CLOCK.2,
        tick_ns_denom: CLOCK.3,
        revision_id: REVISION,
    }
}

fn scratch_dir(name: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "baml-golden-v2-{name}-{}-{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// .bamlseg — session CCT delta stream
// ---------------------------------------------------------------------------

/// Drive a real `SessionWriter` end to end with fixed inputs: one flush
/// window, then a clean seal. Every byte (header, block headers, payloads,
/// crc32c trailers, checkpoint, footer, seal trailer) is deterministic.
fn build_session_bamlseg() -> Vec<u8> {
    let dir = scratch_dir("seg");
    let fsync = FsyncService::start();
    let mut writer = SessionWriter::create(
        &dir,
        EUID,
        ENGINE_ID,
        STARTED_NS,
        CLOCK,
        REVISION,
        "baml_rev_1_golden",
        &fsync,
    )
    .unwrap();
    let mut engine = fixture_engine();
    let flush = engine.flush_window();
    writer
        .write_window(&flush, engine.nodes(), 1_000, 8_000, 8)
        .unwrap();
    writer.close(8_000, "golden").unwrap();

    let cct_dir = dir
        .join("sessions")
        .read_dir()
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let seg = cct_dir
        .path()
        .join("cct")
        .read_dir()
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "bamlseg"))
        .expect("one .bamlseg");
    let bytes = std::fs::read(seg).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    bytes
}

#[test]
fn bamlseg_fixture_is_frozen_and_scans_sealed() {
    let built = build_session_bamlseg();
    assert_golden("session.bamlseg", &built);

    let contents = segment::scan_segment(&built).expect("sealed segment scans");
    assert_eq!(contents.end, ScanEnd::Sealed);
    assert_eq!(contents.header.engine_id, ENGINE_ID);
    assert_eq!(contents.header.process_euid, EUID);
    let kinds: Vec<u8> = contents.blocks.iter().map(|b| b.kind).collect();
    assert!(
        kinds.contains(&(BlockKind::NodeBirth as u8)),
        "birth block present: {kinds:?}"
    );
    assert!(
        kinds.contains(&(BlockKind::CctDelta as u8)),
        "delta block present: {kinds:?}"
    );

    // Delta rows replay to the engine's exact totals.
    let delta = contents
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::CctDelta as u8)
        .unwrap();
    let rows = blocks::decode_cct_delta(delta.payload, delta.row_count as usize).unwrap();
    let enters: u32 = rows.iter().map(|r| r.enters).sum();
    assert_eq!(enters, 3, "three calls entered");
}

#[test]
fn bamlseg_torn_tails_honor_the_recovery_contract() {
    let built = build_session_bamlseg();
    let sealed = segment::scan_segment(&built).unwrap();
    let committed_blocks = sealed.blocks.len();

    // Truncate anywhere inside the final seal: the committed block prefix
    // survives, and the scan reports where the readable bytes end.
    for cut in [built.len() - 1, built.len() - 17, built.len() - 48] {
        let contents = segment::scan_segment(&built[..cut]).expect("torn segment still scans");
        assert_ne!(
            contents.end,
            ScanEnd::Sealed,
            "cut {cut} cannot look sealed"
        );
        assert!(
            contents.blocks.len() <= committed_blocks,
            "cut {cut} grew blocks"
        );
    }

    // Truncating mid-block drops that block but keeps every whole one.
    let half = built.len() / 2;
    let contents = segment::scan_segment(&built[..half]).expect("half segment scans");
    assert!(contents.blocks.len() < committed_blocks);
}

// ---------------------------------------------------------------------------
// .bamlmeta — session + boundary meta streams (BMET)
// ---------------------------------------------------------------------------

fn build_session_bamlmeta() -> Vec<u8> {
    let mut out = meta::encode_header().to_vec();
    for record in [
        MetaRecord::SessionBegin {
            process_euid: EUID,
            engine_id: ENGINE_ID,
            pid: 4242,
            started_epoch_ns: STARTED_NS,
            revision_id: "baml_rev_1_golden".to_string(),
        },
        MetaRecord::SessionHeartbeat {
            wall_epoch_ns: STARTED_NS + 1_000_000_000,
        },
        MetaRecord::SessionEpochClose {
            reason: "segment_budget".to_string(),
            cct_bytes: 4_096,
        },
        MetaRecord::SessionEnd {
            reason: "engine_closed".to_string(),
        },
    ] {
        out.extend_from_slice(&meta::encode_record(&record));
    }
    out
}

fn build_boundary_bamlmeta() -> Vec<u8> {
    let mut out = meta::encode_header().to_vec();
    for record in [
        MetaRecord::BoundaryBegin {
            boundary_id: "baml_id_1_R0lMRElORw".to_string(),
            target: "main".to_string(),
            source: "cli".to_string(),
            created_ms: 1_700_000_000_000,
            project_id: "baml_proj_1_golden".to_string(),
            revision_id: "baml_rev_1_golden".to_string(),
            capture_defaults: "llm_boundary".to_string(),
        },
        MetaRecord::BoundaryBound {
            session_dir: "1700000000-a5a5-e7".to_string(),
            first_seg_seq: 0,
            partition_id: 1,
            boundary_local_id: 1,
        },
        MetaRecord::BoundaryTrigger {
            trigger: "error".to_string(),
            at_ms: 1_700_000_000_500,
            detail: "user.f errored".to_string(),
        },
        MetaRecord::BoundaryLoss {
            kind: "shed".to_string(),
            detail: "ring back-pressure".to_string(),
        },
        MetaRecord::BoundaryComplete {
            status: "succeeded".to_string(),
            completed_ms: 1_700_000_001_000,
            last_seg_seq: 0,
            counts: serde_json::json!({"calls": 3, "errors": 1}),
            diagnostics: vec!["none".to_string()],
            dump_refs: vec![],
        },
    ] {
        out.extend_from_slice(&meta::encode_record(&record));
    }
    out
}

#[test]
fn bamlmeta_fixtures_are_frozen_and_parse() {
    let session = build_session_bamlmeta();
    assert_golden("session.bamlmeta", &session);
    let parsed = meta::read_meta(&session).unwrap();
    assert!(!parsed.truncated);
    assert_eq!(parsed.unknown_records, 0);
    let kinds: Vec<u8> = parsed.records.iter().map(MetaRecord::kind).collect();
    assert_eq!(kinds, vec![1, 2, 3, 4]);

    let boundary = build_boundary_bamlmeta();
    assert_golden("boundary.bamlmeta", &boundary);
    let parsed = meta::read_meta(&boundary).unwrap();
    assert!(!parsed.truncated);
    assert_eq!(parsed.unknown_records, 0);
    let kinds: Vec<u8> = parsed.records.iter().map(MetaRecord::kind).collect();
    assert_eq!(kinds, vec![16, 17, 19, 20, 18]);
}

#[test]
fn bamlmeta_torn_tails_honor_the_recovery_contract() {
    let bytes = build_session_bamlmeta();
    let whole = meta::read_meta(&bytes).unwrap().records.len();
    for cut in [bytes.len() - 1, bytes.len() - 5, bytes.len() / 2] {
        let parsed = meta::read_meta(&bytes[..cut]).expect("torn meta still reads");
        assert!(parsed.truncated, "cut {cut} must flag truncation");
        assert!(parsed.records.len() < whole, "cut {cut} kept all records");
    }
}

// ---------------------------------------------------------------------------
// .bamlcct — sealed folded boundary snapshot
// ---------------------------------------------------------------------------

fn build_boundary_bamlcct() -> Vec<u8> {
    let engine = fixture_engine();
    let partition = engine
        .partition_of_thread(1)
        .expect("root thread has a partition");
    let folded = fold::fold_partition(&engine, partition);
    fold::encode_boundary_snapshot(
        &folded,
        &fixture_header(),
        blocks::PartitionBindRow {
            partition_id: partition,
            boundary_local_id: 1,
            boundary_id: *b"GOLDENBOUNDARY01",
            created_ms: 1_700_000_000_000,
        },
    )
}

#[test]
fn bamlcct_fixture_is_frozen_and_scans_sealed() {
    let built = build_boundary_bamlcct();
    assert_golden("cct.bamlcct", &built);

    let contents = segment::scan_segment(&built).expect("snapshot scans");
    assert_eq!(contents.end, ScanEnd::Sealed);
    let totals = contents
        .blocks
        .iter()
        .find(|b| b.kind == BlockKind::NodeTotal as u8)
        .expect("node_total block");
    let rows = blocks::decode_cct_delta(totals.payload, totals.row_count as usize).unwrap();
    let enters: u32 = rows.iter().map(|r| r.enters).sum();
    let errs: u32 = rows.iter().map(|r| r.ends_err).sum();
    assert_eq!((enters, errs), (3, 1), "folded totals match the program");
}

// ---------------------------------------------------------------------------
// raw firehose container
// ---------------------------------------------------------------------------

fn build_raw_bamlprof() -> Vec<u8> {
    let dir = scratch_dir("raw");
    let mut sink = RawSink::default();
    let range = fixture_records();
    // Two ranges: split the fixture stream mid-way like a real drain.
    let split = range.len() / 2;
    let split = (1..range.len())
        .find(|&i| i >= split && is_record_boundary(&range, i))
        .unwrap_or(range.len());
    sink.push_range(&range[..split]);
    sink.push_range(&range[split..]);
    sink.flush_to(&dir, EUID, ENGINE_ID, CLOCK).unwrap();
    let bytes = std::fs::read(dir.join("raw/raw-000001.bamlprof")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    bytes
}

/// True when `at` falls exactly between two encoded records.
fn is_record_boundary(bytes: &[u8], at: usize) -> bool {
    let mut offset = 0;
    for rec in bex_events::prof::record::iter(bytes) {
        let Ok(rec) = rec else { return false };
        offset += rec.encoded_len();
        if offset == at {
            return true;
        }
        if offset > at {
            return false;
        }
    }
    false
}

#[test]
fn raw_fixture_is_frozen_and_replays() {
    let built = build_raw_bamlprof();
    assert_golden("raw-000001.bamlprof", &built);

    let parsed = raw::read_raw_file(&built).unwrap();
    assert_eq!(parsed.process_euid, EUID);
    assert_eq!(parsed.engine_id, ENGINE_ID);
    assert_eq!(parsed.clock, CLOCK);
    assert_eq!(parsed.ranges.len(), 2);
    assert_eq!(parsed.torn_bytes, 0);

    // The framed ranges replay into the exact CCT the live engine built.
    let mut replay = CctEngine::new(32);
    for range in &parsed.ranges {
        replay.consume(range, &mut |t| t);
    }
    let live = fixture_engine();
    assert_eq!(replay.nodes().len(), live.nodes().len());
}

#[test]
fn raw_torn_tails_honor_the_recovery_contract() {
    let built = build_raw_bamlprof();
    for cut in [built.len() - 1, built.len() - 3] {
        let parsed = raw::read_raw_file(&built[..cut]).expect("torn raw still reads");
        assert_eq!(parsed.ranges.len(), 1, "cut {cut} keeps whole frames only");
        assert!(parsed.torn_bytes > 0);
    }
}

// ---------------------------------------------------------------------------
// C9: canonical value encoding + CIDs (§7.4) — the frozen dedupe contract
// ---------------------------------------------------------------------------

fn build_canon_fixture() -> Vec<u8> {
    use bex_events::store::canon::{CanonValue, Presence, encode};
    // One value exercising every tag: scalars, normalized floats/bigints,
    // canonical map order, class with presence bytes, enum, media,
    // omitted, a chunked string, and a 128-ary segmented list.
    let value = CanonValue::Map(vec![
        ("zz".into(), CanonValue::Bool(true)),
        ("aa".into(), CanonValue::Int(-7)),
        ("nan".into(), CanonValue::Float(f64::NAN)),
        ("big".into(), CanonValue::Bigint("+00042".into())),
        (
            "cls".into(),
            CanonValue::Class {
                definition_key: "class:user.Box".into(),
                fields: vec![
                    ("width".into(), Presence::Value, Some(CanonValue::Int(3))),
                    ("label".into(), Presence::Null, None),
                ],
            },
        ),
        (
            "enm".into(),
            CanonValue::Enum {
                definition_key: "enum:user.Color".into(),
                variant: "Red".into(),
            },
        ),
        (
            "med".into(),
            CanonValue::Media {
                kind: "image".into(),
                mime: Some("image/png".into()),
                content_kind: 0,
                content: "https://example.test/x.png".into(),
            },
        ),
        (
            "omt".into(),
            CanonValue::Omitted {
                reason: 4,
                message: "CyclicReference".into(),
            },
        ),
        ("long".into(), CanonValue::String("p".repeat(200_000))),
        (
            "wide".into(),
            CanonValue::List((0..300).map(CanonValue::Int).collect()),
        ),
    ]);
    let encoded = encode(&value);
    // Serialize the whole DAG deterministically: root CID, then every
    // node/chunk (cid + len + bytes) in emission order.
    let mut out = Vec::new();
    out.extend_from_slice(&encoded.root_cid);
    out.extend_from_slice(&encoded.logical_len.to_le_bytes());
    for (cid, bytes) in encoded.nodes.iter().chain(encoded.chunks.iter()) {
        out.extend_from_slice(cid);
        out.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

#[test]
fn canon_cids_are_frozen() {
    let built = build_canon_fixture();
    assert_golden("canon.bamlcanon", &built);
    // The root CID doubles as a compact drift alarm in this test's output.
    let root: [u8; 32] = built[0..32].try_into().unwrap();
    let wire = bex_events::store::canon::cid_wire(&root);
    assert!(wire.starts_with("bamlv_1_"), "{wire}");
}

#[test]
fn canon_fixture_decodes_and_reencodes_to_the_same_root() {
    use bex_events::store::canon::{self, CanonValue, DagSource};
    use rustc_hash::FxHashMap;

    // Fixture layout (build_canon_fixture): root_cid + logical_len u64 +
    // repeated [cid, len u32, bytes] for every node/chunk in emission
    // order. CIDs are domain-separated, so one map serves both lookups.
    struct FixtureSource(FxHashMap<[u8; 32], Vec<u8>>);
    impl DagSource for FixtureSource {
        fn node(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>> {
            self.0.get(cid).cloned()
        }
        fn chunk(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>> {
            self.0.get(cid).cloned()
        }
    }

    let bytes = std::fs::read(golden_dir().join("canon.bamlcanon")).expect("fixture exists");
    let root_cid: [u8; 32] = bytes[0..32].try_into().unwrap();
    let mut entries = FxHashMap::default();
    let mut at = 40; // skip root cid + logical_len
    while at < bytes.len() {
        let cid: [u8; 32] = bytes[at..at + 32].try_into().unwrap();
        let len = u32::from_le_bytes(bytes[at + 32..at + 36].try_into().unwrap()) as usize;
        entries.insert(cid, bytes[at + 36..at + 36 + len].to_vec());
        at += 36 + len;
    }
    let mut src = FixtureSource(entries);
    let root_bytes = src.0[&root_cid].clone();

    let decoded = canon::decode(&root_bytes, &mut src).expect("frozen fixture decodes");

    // Structural spot-checks against the fixture builder's value.
    let CanonValue::Map(ref entries) = decoded else {
        panic!("fixture root is a map")
    };
    let get = |k: &str| entries.iter().find(|(key, _)| key == k).map(|(_, v)| v);
    assert_eq!(get("aa"), Some(&CanonValue::Int(-7)));
    assert!(matches!(get("nan"), Some(CanonValue::Float(f)) if f.is_nan()));
    assert_eq!(get("big"), Some(&CanonValue::Bigint("42".to_string())));
    assert!(
        matches!(get("long"), Some(CanonValue::String(s)) if s.len() == 200_000),
        "chunked string reassembles"
    );
    assert!(
        matches!(get("wide"), Some(CanonValue::List(items)) if items.len() == 300),
        "segmented list splices"
    );

    // The inverse proof: re-encoding the decoded value reproduces the
    // exact frozen root CID.
    let re = canon::encode(&decoded);
    assert_eq!(
        re.root_cid, root_cid,
        "decode∘encode is the identity on the fixture"
    );
}
