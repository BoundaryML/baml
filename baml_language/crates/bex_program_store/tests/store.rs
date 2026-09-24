// Tests of the program store through its public interface: the entry file
// layout, verification on read, runtime builds, concurrent writers in threads
// and in separate processes, hostile hash arguments, and lookup cost on a
// large store.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use bex_program_store::{
    Durability, FORMAT_VERSION, Lookup, MAGIC, Missing, ProgramHash, ProgramStore, StoreError,
};

const BUILD: &str = "1.2.3+abc";

/// A payload that is large enough to be written in several chunks, so that a
/// torn write would be visible to a concurrent reader.
fn payload(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| seed.wrapping_add(u8::try_from(i % 251).unwrap()))
        .collect()
}

fn open(dir: &Path) -> ProgramStore {
    ProgramStore::open(dir, BUILD)
        .unwrap()
        .with_durability(Durability::None)
}

fn missing(lookup: Lookup) -> Missing {
    match lookup {
        Lookup::Missing(missing) => missing,
        Lookup::Found(found) => panic!("expected a missing entry, found {found:?}"),
    }
}

/// The documented layout, written by hand. The test that reads this entry
/// fails when the layout changes without a format version bump.
fn hand_written_entry(format: u32, builds: &str, payload: &[u8]) -> Vec<u8> {
    let mut entry = Vec::new();
    entry.extend_from_slice(b"BAMLPROG");
    entry.extend_from_slice(&format.to_le_bytes());
    entry.extend_from_slice(&u32::try_from(builds.len()).unwrap().to_le_bytes());
    entry.extend_from_slice(builds.as_bytes());
    entry.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    entry.extend_from_slice(payload);
    entry
}

fn write_raw(store: &ProgramStore, hash: ProgramHash, entry: &[u8]) -> PathBuf {
    let path = store.entry_path(hash);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, entry).unwrap();
    path
}

#[test]
fn put_then_get_round_trips_and_uses_the_contract_layout() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let bytes = payload(1, 100_000);
    let hash = ProgramHash::of(&bytes);

    assert!(matches!(missing(store.get(hash)), Missing::NotFound));

    let outcome = store.put(&bytes).unwrap();
    assert_eq!(outcome.hash, hash);
    assert!(outcome.written);

    let hex = hash.to_hex();
    let expected_path = dir.path().join(&hex[..2]).join(format!("{hex}.bamlprog"));
    assert_eq!(store.entry_path(hash), expected_path);
    assert!(expected_path.is_file());

    let found = store.get(hash).found().expect("entry after put");
    assert_eq!(found.hash(), hash);
    assert_eq!(found.bytes(), &bytes[..]);
    assert_eq!(found.written_by(), BUILD);
    assert_eq!(found.into_bytes(), bytes);

    // Immutable: a second put of the same bytes writes nothing.
    let modified = std::fs::metadata(&expected_path)
        .unwrap()
        .modified()
        .unwrap();
    let again = store.put(&bytes).unwrap();
    assert!(!again.written);
    assert_eq!(
        std::fs::metadata(&expected_path)
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );

    // The shard holds the entry and the marker of this build, and no
    // temporary file.
    let mut files: Vec<_> = std::fs::read_dir(expected_path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    let mut expected_files = vec![expected_path, store.marker_path(hash, BUILD)];
    expected_files.sort();
    assert_eq!(files, expected_files);
    assert_eq!(
        std::fs::read_to_string(store.marker_path(hash, BUILD)).unwrap(),
        BUILD
    );
}

#[test]
fn the_hash_is_the_sha256_of_the_payload_only() {
    // SHA-256 of the empty input, a fixed point of reference that does not
    // depend on this crate.
    let empty = ProgramHash::of(b"");
    assert_eq!(
        empty.to_hex(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    assert_eq!(store.put(b"").unwrap().hash, empty);
    assert_eq!(store.get(empty).found().unwrap().bytes(), b"");
}

#[test]
fn a_hand_written_entry_in_the_documented_layout_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let bytes = payload(2, 5000);
    let hash = ProgramHash::of(&bytes);
    assert_eq!(MAGIC, *b"BAMLPROG");
    write_raw(
        &store,
        hash,
        &hand_written_entry(FORMAT_VERSION, BUILD, &bytes),
    );
    assert_eq!(store.get(hash).found().unwrap().bytes(), &bytes[..]);

    // And the writer produces exactly that layout.
    let other = tempfile::tempdir().unwrap();
    let written = open(other.path());
    written.put(&bytes).unwrap();
    assert_eq!(
        std::fs::read(written.entry_path(hash)).unwrap(),
        hand_written_entry(FORMAT_VERSION, BUILD, &bytes)
    );
}

#[test]
fn truncated_torn_and_altered_entries_are_missing_and_the_next_put_repairs_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let bytes = payload(3, 50_000);
    let hash = store.put(&bytes).unwrap().hash;
    let path = store.entry_path(hash);
    let good = std::fs::read(&path).unwrap();

    // Every prefix length class: empty, inside the fixed prefix, inside the
    // build list, inside the payload length, inside the payload, one short.
    for cut in [0, 3, 10, 18, 27, 1000, good.len() - 1] {
        std::fs::write(&path, &good[..cut]).unwrap();
        assert!(
            matches!(missing(store.get(hash)), Missing::Corrupt(_)),
            "a file cut to {cut} bytes must be corrupt"
        );
    }

    // One flipped payload bit.
    let mut flipped = good.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0x01;
    std::fs::write(&path, &flipped).unwrap();
    assert!(matches!(missing(store.get(hash)), Missing::Corrupt(_)));

    // Trailing bytes after the payload.
    let mut longer = good.clone();
    longer.push(0);
    std::fs::write(&path, &longer).unwrap();
    assert!(matches!(missing(store.get(hash)), Missing::Corrupt(_)));

    // A wrong magic.
    let mut magic = good.clone();
    magic[0] = b'X';
    std::fs::write(&path, &magic).unwrap();
    assert!(matches!(missing(store.get(hash)), Missing::Corrupt(_)));

    // A header that announces an absurd build name must not allocate for it.
    let mut absurd = good.clone();
    absurd[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&path, &absurd).unwrap();
    assert!(matches!(missing(store.get(hash)), Missing::Corrupt(_)));

    // A directory in place of the file.
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(matches!(
        missing(store.get(hash)),
        Missing::Unreadable(_) | Missing::Corrupt(_)
    ));
    std::fs::remove_dir(&path).unwrap();

    // The next put replaces a corrupt entry.
    std::fs::write(&path, &good[..1000]).unwrap();
    assert!(store.put(&bytes).unwrap().written);
    assert_eq!(store.get(hash).found().unwrap().bytes(), &bytes[..]);
}

#[test]
fn an_entry_stored_under_another_hash_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let program_a = payload(4, 4000);
    let program_b = payload(5, 4000);
    let hash_a = ProgramHash::of(&program_a);
    let hash_b = store.put(&program_b).unwrap().hash;

    // A valid entry for B, copied to the path of A.
    let entry_b = std::fs::read(store.entry_path(hash_b)).unwrap();
    write_raw(&store, hash_a, &entry_b);

    match missing(store.get(hash_a)) {
        Missing::Corrupt(reason) => assert!(reason.contains(&hash_b.to_hex()), "{reason}"),
        other => panic!("expected a corrupt entry, got {other:?}"),
    }
    assert!(matches!(
        missing(store.get_any_build(hash_a)),
        Missing::Corrupt(_)
    ));
}

#[test]
fn put_received_refuses_bytes_with_another_hash_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let claimed = ProgramHash::of(b"what the sender promised");
    let error = store
        .put_received(claimed, b"what arrived", BUILD)
        .unwrap_err();
    match error {
        StoreError::HashMismatch { expected, actual } => {
            assert_eq!(expected, claimed);
            assert_eq!(actual, ProgramHash::of(b"what arrived"));
        }
        other => panic!("{other}"),
    }
    assert_eq!(store.entries().count(), 0);

    let bytes = b"what the sender promised";
    assert!(store.put_received(claimed, bytes, BUILD).unwrap().written);
    assert_eq!(store.get(claimed).found().unwrap().bytes(), bytes);
}

#[test]
fn entries_of_another_runtime_build_are_missing_until_this_build_stores_the_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let old = ProgramStore::open(dir.path(), "0.9.0")
        .unwrap()
        .with_durability(Durability::None);
    let new = open(dir.path());
    let bytes = payload(6, 20_000);
    let hash = old.put(&bytes).unwrap().hash;

    match missing(new.get(hash)) {
        Missing::OtherBuild { written_by } => assert_eq!(written_by, "0.9.0"),
        other => panic!("expected OtherBuild, got {other:?}"),
    }
    // Forwarding to another site still works, and reports the build.
    let any = new.get_any_build(hash).found().unwrap();
    assert_eq!(any.written_by(), "0.9.0");

    // The new build compiled the same bytes: it records that next to the
    // entry, and the entry itself is not rewritten. Two builds that share a
    // store during a rolling deploy therefore do not evict each other.
    let entry_before = std::fs::read(new.entry_path(hash)).unwrap();
    assert!(new.put(&bytes).unwrap().written);
    assert_eq!(std::fs::read(new.entry_path(hash)).unwrap(), entry_before);
    assert_eq!(new.get(hash).found().unwrap().bytes(), &bytes[..]);
    assert_eq!(old.get(hash).found().unwrap().bytes(), &bytes[..]);
    assert!(!old.put(&bytes).unwrap().written);
    assert!(!new.put(&bytes).unwrap().written);

    // Bytes received from a site that runs another build are kept under that
    // build and stay invisible to this one.
    let foreign = payload(7, 3000);
    let foreign_hash = ProgramHash::of(&foreign);
    new.put_received(foreign_hash, &foreign, "0.9.0").unwrap();
    assert!(matches!(
        missing(new.get(foreign_hash)),
        Missing::OtherBuild { .. }
    ));
    assert!(old.get(foreign_hash).found().is_some());
}

#[test]
fn an_entry_whose_header_names_this_build_needs_no_marker() {
    // A writer that only knows the entry layout of the contract (a site
    // server that received a program over HTTP) writes no marker.
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let bytes = payload(8, 100);
    let hash = ProgramHash::of(&bytes);
    write_raw(
        &store,
        hash,
        &hand_written_entry(FORMAT_VERSION, BUILD, &bytes),
    );
    assert!(!store.marker_path(hash, BUILD).exists());
    assert_eq!(store.get(hash).found().unwrap().bytes(), &bytes[..]);

    // A marker alone is not an entry.
    let lonely = ProgramHash::of(b"never stored");
    let marker = store.marker_path(lonely, BUILD);
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, BUILD).unwrap();
    assert!(matches!(missing(store.get(lonely)), Missing::NotFound));
}

#[test]
fn many_builds_record_the_same_entry_independently() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = payload(8, 100);
    let hash = ProgramHash::of(&bytes);
    let stores: Vec<_> = (0..40)
        .map(|n| {
            ProgramStore::open(dir.path(), format!("build-{n}"))
                .unwrap()
                .with_durability(Durability::None)
        })
        .collect();
    for store in &stores {
        store.put(&bytes).unwrap();
    }
    for store in &stores {
        assert!(
            store.get(hash).found().is_some(),
            "{}",
            store.runtime_build()
        );
    }
    // Build names that are unfit for file names get a tag instead.
    let odd = ProgramStore::open(dir.path(), "0.1.0+sha/with:odd*chars ..")
        .unwrap()
        .with_durability(Durability::None);
    odd.put(&bytes).unwrap();
    assert!(odd.get(hash).found().is_some());
    assert_eq!(
        odd.marker_path(hash, odd.runtime_build()).parent(),
        odd.entry_path(hash).parent()
    );
}

#[test]
fn an_unknown_store_format_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let bytes = payload(9, 100);
    let hash = ProgramHash::of(&bytes);
    write_raw(
        &store,
        hash,
        &hand_written_entry(FORMAT_VERSION + 1, BUILD, &bytes),
    );
    assert!(matches!(
        missing(store.get(hash)),
        Missing::UnsupportedFormat(version) if version == FORMAT_VERSION + 1
    ));
    // This build replaces it with an entry it can read.
    assert!(store.put(&bytes).unwrap().written);
    assert!(store.get(hash).found().is_some());
}

#[test]
fn unusable_build_names_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    for build in [
        "",
        "a\nb",
        "a\rb",
        &"x".repeat(bex_program_store::MAX_BUILD_LEN + 1),
    ] {
        assert!(
            matches!(
                ProgramStore::open(dir.path(), build),
                Err(StoreError::InvalidBuild(_))
            ),
            "{build:?}"
        );
    }
    let store = open(dir.path());
    assert!(matches!(
        store.put_received(ProgramHash::of(b"x"), b"x", "two\nlines"),
        Err(StoreError::InvalidBuild(_))
    ));
}

#[test]
fn a_hash_argument_cannot_name_a_path() {
    let good = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_eq!(good.parse::<ProgramHash>().unwrap().to_hex(), good);

    let traversal_64 = format!("../../{}", &good[6..]);
    assert_eq!(traversal_64.len(), 64);
    let hostile = [
        "",
        "..",
        "../../etc/passwd",
        "/etc/passwd",
        traversal_64.as_str(),
        // Right length, wrong alphabet.
        &good.to_uppercase(),
        &good.replace('e', "g"),
        &good.replace('e', "/"),
        &good.replace('e', "\\"),
        &good.replace('e', "."),
        &good.replace('e', "\0"),
        // Wrong length.
        &good[..63],
        &format!("{good}0"),
        &format!(" {good}"),
        &format!("{good}\n"),
        &format!("{good}.bamlprog"),
        // Multi-byte characters that add up to 64 bytes.
        &"é".repeat(32),
    ];
    for text in hostile {
        assert!(
            matches!(text.parse::<ProgramHash>(), Err(StoreError::InvalidHash(_))),
            "{text:?} must not parse"
        );
    }

    // Every path the store derives from a hash stays inside the store.
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    for seed in 0..=255u8 {
        let path = store.entry_path(ProgramHash::from_bytes([seed; 32]));
        let relative = path.strip_prefix(dir.path()).unwrap();
        assert_eq!(relative.components().count(), 2, "{relative:?}");
        assert!(
            relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        );
    }
}

#[test]
fn concurrent_writers_and_readers_in_threads_never_see_a_partial_entry() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = payload(10, 2_000_000);
    let hash = ProgramHash::of(&bytes);
    let stop = std::sync::atomic::AtomicBool::new(false);

    std::thread::scope(|scope| {
        let readers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let store = open(dir.path());
                    let mut hits = 0u32;
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        match store.get_any_build(hash) {
                            Lookup::Found(found) => {
                                assert_eq!(found.bytes().len(), bytes.len());
                                hits += 1;
                            }
                            Lookup::Missing(Missing::NotFound) => {}
                            Lookup::Missing(other) => panic!("a reader saw {other}"),
                        }
                    }
                    hits
                })
            })
            .collect();

        let writers: Vec<_> = (0..8)
            .map(|n| {
                let bytes = &bytes;
                let dir = dir.path();
                scope.spawn(move || {
                    // Two builds, so that the merge path races too.
                    let store = ProgramStore::open(dir, format!("build-{}", n % 2))
                        .unwrap()
                        .with_durability(Durability::None);
                    for _ in 0..5 {
                        assert_eq!(store.put(bytes).unwrap().hash, hash);
                        assert!(store.get(hash).found().is_some());
                    }
                })
            })
            .collect();
        // Stop the readers before any panic is propagated. A reader that
        // keeps spinning would keep the scope, and the test, alive forever.
        let writers: Vec<_> = writers
            .into_iter()
            .map(std::thread::ScopedJoinHandle::join)
            .collect();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let readers: Vec<_> = readers
            .into_iter()
            .map(std::thread::ScopedJoinHandle::join)
            .collect();
        for result in writers {
            result.expect("a writer failed");
        }
        for result in readers {
            assert!(
                result.expect("a reader failed") > 0,
                "a reader never saw the entry"
            );
        }
    });

    // Both builds are served, and the shard holds the entry, the two markers,
    // and no temporary file.
    for build in ["build-0", "build-1"] {
        let store = ProgramStore::open(dir.path(), build).unwrap();
        assert_eq!(
            store.get(hash).found().expect(build).bytes().len(),
            bytes.len()
        );
    }
    let shard = open(dir.path()).entry_path(hash);
    assert_eq!(
        std::fs::read_dir(shard.parent().unwrap()).unwrap().count(),
        3
    );
}

const CHILD_DIR: &str = "BEX_PROGRAM_STORE_TEST_CHILD_DIR";
const CHILD_BUILD: &str = "BEX_PROGRAM_STORE_TEST_CHILD_BUILD";

fn process_test_payloads() -> Vec<Vec<u8>> {
    (0..6).map(|seed| payload(100 + seed, 600_000)).collect()
}

/// Body of the child processes of the next test. Without the environment
/// variable it is an empty test.
#[test]
fn child_process_writer() {
    let Some(dir) = std::env::var_os(CHILD_DIR) else {
        return;
    };
    let build = std::env::var(CHILD_BUILD).unwrap();
    // Full durability: the flushes widen the window between the write and the
    // rename, which is where writers of separate processes interleave.
    let store = ProgramStore::open(PathBuf::from(dir), build).unwrap();
    for _ in 0..3 {
        for bytes in process_test_payloads() {
            let hash = store.put(&bytes).unwrap().hash;
            let found = store.get(hash).found().expect("own entry after put");
            assert_eq!(found.bytes(), &bytes[..]);
        }
    }
}

#[test]
fn concurrent_writers_in_separate_processes_converge() {
    let dir = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().unwrap();
    let children: Vec<_> = (0..6)
        .map(|n| {
            Command::new(&exe)
                .args(["child_process_writer", "--exact", "--nocapture"])
                .env(CHILD_DIR, dir.path())
                .env(CHILD_BUILD, format!("proc-build-{}", n % 3))
                .spawn()
                .unwrap()
        })
        .collect();
    for mut child in children {
        assert!(child.wait().unwrap().success(), "a writer process failed");
    }

    let store = open(dir.path());
    let payloads = process_test_payloads();
    for bytes in &payloads {
        let found = store
            .get_any_build(ProgramHash::of(bytes))
            .found()
            .expect("every program is stored");
        assert_eq!(found.bytes(), &bytes[..]);
        for build in ["proc-build-0", "proc-build-1", "proc-build-2"] {
            let as_build = ProgramStore::open(dir.path(), build).unwrap();
            assert!(as_build.get(found.hash()).found().is_some(), "{build}");
        }
    }
    let listed: Vec<_> = store.entries().map(Result::unwrap).collect();
    assert_eq!(listed.len(), payloads.len());
    assert_eq!(store.remove_stale_temp_files(Duration::ZERO), 0);
}

#[test]
fn ten_thousand_entries_keep_lookups_fast() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let small = |n: u32| format!("synthetic program {n}").into_bytes();

    let empty_store_lookup = {
        let one = tempfile::tempdir().unwrap();
        let one_store = open(one.path());
        let hash = one_store.put(&small(0)).unwrap().hash;
        let started = Instant::now();
        for _ in 0..2000 {
            assert!(one_store.get(hash).found().is_some());
        }
        started.elapsed()
    };

    let hashes: Vec<ProgramHash> = (0..10_000)
        .map(|n| store.put(&small(n)).unwrap().hash)
        .collect();

    // The fan-out bounds every directory: 10,000 entries and their markers
    // over 256 shards, about 78 files per shard.
    let largest_shard = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|shard| std::fs::read_dir(shard.unwrap().path()).unwrap().count())
        .max()
        .unwrap();
    assert!(
        largest_shard < 200,
        "largest shard holds {largest_shard} files"
    );

    let started = Instant::now();
    for (n, hash) in hashes.iter().step_by(5).enumerate() {
        let found = store.get(*hash).found().expect("hit");
        assert_eq!(found.bytes(), &small(u32::try_from(n).unwrap() * 5)[..]);
    }
    for n in 0..2000u32 {
        let absent = ProgramHash::of(format!("absent {n}").as_bytes());
        assert!(matches!(missing(store.get(absent)), Missing::NotFound));
    }
    let full_store_lookup = started.elapsed();

    // 4000 lookups against 2000 in the reference. The bound is loose on
    // purpose: it fails when a lookup starts to scan, not on a slow machine.
    assert!(
        full_store_lookup < empty_store_lookup * 10 + Duration::from_millis(500),
        "4000 lookups in a store of 10,000 took {full_store_lookup:?}; \
         2000 lookups in a store of 1 took {empty_store_lookup:?}"
    );

    // The collector interface sees every entry exactly once, without holding
    // a list of them.
    let mut listed = 0;
    for entry in store.entries() {
        let entry = entry.unwrap();
        assert_eq!(store.entry_path(entry.hash), entry.path);
        listed += 1;
    }
    assert_eq!(listed, 10_000);
}

#[test]
fn listing_skips_foreign_files_and_remove_deletes_one_entry() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    assert_eq!(
        store.entries().count(),
        0,
        "a store that does not exist yet"
    );

    let first = store.put(b"first").unwrap().hash;
    let second = store.put(b"second").unwrap().hash;

    // Files that are not entries: a note, an entry in the wrong shard, a
    // temporary file of a dead writer, and a directory outside the fan-out.
    let shard = store.entry_path(first).parent().unwrap().to_path_buf();
    std::fs::write(shard.join("README.txt"), b"x").unwrap();
    let wrong_shard = dir.path().join(if first.to_hex().starts_with("00") {
        "01"
    } else {
        "00"
    });
    std::fs::create_dir_all(&wrong_shard).unwrap();
    std::fs::write(wrong_shard.join(format!("{first}.bamlprog")), b"x").unwrap();
    std::fs::write(shard.join(format!(".tmp-{first}.1.0.0.partial")), b"x").unwrap();
    std::fs::create_dir_all(dir.path().join("not-a-shard")).unwrap();

    let mut listed: Vec<_> = store.entries().map(|entry| entry.unwrap().hash).collect();
    listed.sort();
    let mut expected = vec![first, second];
    expected.sort();
    assert_eq!(listed, expected);

    // A young temporary file is kept, an old one is deleted.
    assert_eq!(store.remove_stale_temp_files(Duration::from_secs(3600)), 0);
    assert_eq!(store.remove_stale_temp_files(Duration::ZERO), 1);

    assert!(store.marker_path(first, BUILD).is_file());
    assert!(store.remove(first).unwrap());
    assert!(
        !store.marker_path(first, BUILD).exists(),
        "markers go with the entry"
    );
    assert!(store.marker_path(second, BUILD).is_file());
    assert!(!store.remove(first).unwrap(), "already removed");
    assert!(matches!(missing(store.get(first)), Missing::NotFound));
    assert!(store.get(second).found().is_some());
}

#[test]
fn a_put_into_an_unwritable_store_reports_the_path() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the store directory should be.
    let blocked = dir.path().join("store");
    std::fs::write(&blocked, b"not a directory").unwrap();
    let store = open(&blocked);
    match store.put(b"program").unwrap_err() {
        StoreError::Io { path, .. } => assert!(path.starts_with(&blocked), "{path:?}"),
        other => panic!("{other}"),
    }
    assert!(matches!(
        store.get(ProgramHash::of(b"program")),
        Lookup::Missing(_)
    ));
}

/// An entry whose header names this build is usable without a marker (a site
/// server writes it that way). A `put` of the same bytes must then succeed
/// without writing anything, also when the store cannot be written.
#[cfg(unix)]
#[test]
fn a_put_of_a_usable_entry_succeeds_on_a_read_only_store() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let bytes = payload(21, 300);
    let hash = ProgramHash::of(&bytes);
    let entry = write_raw(
        &store,
        hash,
        &hand_written_entry(FORMAT_VERSION, BUILD, &bytes),
    );
    let shard = entry.parent().unwrap().to_path_buf();
    let read_only = |mode: u32| {
        std::fs::set_permissions(&shard, std::fs::Permissions::from_mode(mode)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(mode)).unwrap();
    };
    read_only(0o555);
    let outcome = store.put(&bytes);
    let lookup = store.get(hash);
    // Restore the permissions before any assertion, so that the temporary
    // directory can be removed.
    read_only(0o755);
    let outcome = outcome.expect("the entry is already usable");
    assert_eq!(outcome.hash, hash);
    assert!(!outcome.written);
    assert_eq!(lookup.found().unwrap().bytes(), &bytes[..]);
    assert!(!store.marker_path(hash, BUILD).exists());
}
