//! The type-shapes fixture compiled to a native library and called through
//! bindings shaped like generated code (`type_shapes/bindings.rs`).
//!
//! The library is the fixture of today's SDK tests
//! (`sdk_tests/fixtures/type_shapes/baml_src`) with one more namespace,
//! `bridge_test` (`type_shapes/bridge_test.baml`), compiled with the
//! exports the bindings were written for. One library per process: the
//! tests share it and take turns. The same round trips run again in a
//! process of their own collecting on every allocation.
#![cfg(all(target_arch = "aarch64", target_vendor = "apple"))]
#![allow(
    clippy::print_stderr,
    clippy::float_cmp,
    reason = "tests that say what they saw, and that a float comes back bit for bit"
)]

#[path = "type_shapes/bindings.rs"]
mod bindings;
#[path = "type_shapes/profile.rs"]
mod profile;

use std::{
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
    time::Duration,
};

use baml_compiler2_native::{CompileOptions, compile_library, export_name, link_library};
use baml_db::{
    ProjectDatabase, SourceRootKind, SourceRootSpec, baml_compiler2_hir::item_data::file_functions,
};
use baml_native_host::{Error, Library, SdkError};
use bindings::{
    Map, bridge_test, class_refs, complex_models, enums, lists, maps, optional, primitives, unions,
};

/// The exports, in the order their numbers in the bindings say.
const EXPORTS: [&str; 39] = [
    "bridge_test.add",
    "bridge_test.lookup",
    "bridge_test.round_trip_points",
    "bridge_test.make_points",
    "bridge_test.count_points",
    "bridge_test.make_ints",
    "bridge_test.count_ints",
    "bridge_test.nap",
    "bridge_test.env_or",
    "make_foo",
    "round_trip_foo",
    "class_refs.make_outer",
    "class_refs.round_trip_inner",
    "class_refs.round_trip_outer",
    "complex_models.round_trip_complex_profile",
    "enums.pick_sentiment",
    "enums.round_trip_sentiment",
    "enums.round_trip_enums",
    "lists.round_trip_ints",
    "lists.round_trip_optional_strings",
    "lists.round_trip_union_list",
    "lists.round_trip_list_container",
    "maps.round_trip_simple_map",
    "maps.round_trip_enum_keyed_map",
    "maps.round_trip_list_valued_map",
    "maps.round_trip_map_container",
    "optional.round_trip_optional_int",
    "optional.round_trip_optional_resume",
    "optional.round_trip_optional_union",
    "optional.round_trip_optional_container",
    "unions.round_trip_null_to_end",
    "unions.round_trip_union_container",
    "unions.round_trip_str_or_int_list",
    "unions.round_trip_optional_plus_null",
    "primitives.round_trip_int",
    "primitives.round_trip_float",
    "primitives.round_trip_string",
    "primitives.round_trip_bool",
    "primitives.return_null",
];

/// The fixture and `bridge_test`, compiled and linked into a library once
/// per process. Its path.
fn library_path() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        if let Some(path) = std::env::var_os("BAML_NATIVE_TEST_LIBRARY") {
            return PathBuf::from(path);
        }
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sdk_tests/fixtures/type_shapes/baml_src")
            .canonicalize()
            .expect("the fixture");
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let root = db
            .add_source_root(SourceRootSpec::new(&fixture, SourceRootKind::Workspace))
            .expect("a source root");
        let mut files = Vec::new();
        let mut pending = vec![fixture.clone()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).expect("the fixture") {
                let entry = entry.expect("an entry").path();
                if entry.is_dir() {
                    pending.push(entry);
                } else if entry
                    .extension()
                    .is_some_and(|extension| extension == "baml")
                {
                    files.push(entry);
                }
            }
        }
        files.push(fixture.join("ns_bridge_test/types.baml"));
        files.sort();
        for file in files {
            let text = if file.ends_with("ns_bridge_test/types.baml") {
                include_str!("type_shapes/bridge_test.baml").to_owned()
            } else {
                std::fs::read_to_string(&file).expect("a fixture file")
            };
            db.add_or_update_file_in(root, &file, &text);
        }
        let functions: Vec<_> = db
            .workspace_files()
            .into_iter()
            .flat_map(|file| file_functions(&db, file).iter().copied())
            .collect();
        let exports: Vec<_> = EXPORTS
            .iter()
            .map(|name| {
                *functions
                    .iter()
                    .find(|function| export_name(&db, **function) == *name)
                    .unwrap_or_else(|| panic!("no function {name}"))
            })
            .collect();
        let compiled = compile_library(&db, &exports, CompileOptions::default())
            .unwrap_or_else(|error| panic!("the fixture compiles: {error}"));
        let directory = tempfile::tempdir().expect("a directory").keep();
        let path = directory.join("libtype_shapes.dylib");
        link_library(&compiled.object, &path).expect("the library links");
        path
    })
}

/// The library, for one test at a time.
fn library() -> (MutexGuard<'static, ()>, &'static Library) {
    static TURNS: Mutex<()> = Mutex::new(());
    let turn = TURNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = library_path().to_str().expect("UTF-8").to_owned();
    let library = bindings::_runtime::init(&path).expect("the library loads");
    (turn, library)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .build()
        .expect("a runtime")
}

/// Every round trip of the fixture returns what went in.
async fn every_round_trip() {
    let profile = profile::complex_profile();
    assert_eq!(
        complex_models::round_trip_complex_profile(&profile)
            .await
            .expect("round trip"),
        profile
    );

    let root = bindings::Foo { v: 7 };
    assert_eq!(bindings::round_trip_foo(&root).await.expect("foo"), root);
    assert_eq!(
        bindings::make_foo(3).await.expect("make"),
        bindings::Foo { v: 3 }
    );

    let outer = class_refs::Outer {
        inner: class_refs::Inner { value: 9 },
    };
    assert_eq!(
        class_refs::round_trip_outer(&outer).await.expect("outer"),
        outer
    );
    assert_eq!(class_refs::make_outer(9).await.expect("make"), outer);
    assert_eq!(
        class_refs::round_trip_inner(class_refs::Inner { value: -4 })
            .await
            .expect("inner"),
        class_refs::Inner { value: -4 }
    );

    assert_eq!(
        enums::pick_sentiment(false).await.expect("pick"),
        enums::Sentiment::Negative
    );
    assert_eq!(
        enums::round_trip_sentiment(enums::Sentiment::Positive)
            .await
            .expect("sentiment"),
        enums::Sentiment::Positive
    );
    let both = enums::Enums {
        bare_enum: enums::Sentiment::Negative,
        variant_as_type: enums::Sentiment::Positive,
    };
    assert_eq!(enums::round_trip_enums(&both).await.expect("enums"), both);

    assert_eq!(
        lists::round_trip_ints(vec![1, -2, 3]).await.expect("ints"),
        [1, -2, 3]
    );
    assert_eq!(
        lists::round_trip_ints(vec![]).await.expect("none"),
        Vec::<i64>::new()
    );
    let strings = vec![Some("a".to_owned()), None, Some(String::new())];
    assert_eq!(
        lists::round_trip_optional_strings(strings.clone())
            .await
            .expect("strings"),
        strings
    );
    let mixed = vec![
        lists::IntOrString::Int(1),
        lists::IntOrString::String("two".to_owned()),
    ];
    assert_eq!(
        lists::round_trip_union_list(mixed.clone())
            .await
            .expect("mixed"),
        mixed
    );
    let container = lists::ListContainer {
        ints: vec![4, 5],
        optional_strings: vec![None, Some("x".to_owned())],
        union_list: mixed,
    };
    assert_eq!(
        lists::round_trip_list_container(&container)
            .await
            .expect("lists"),
        container
    );

    let simple = Map::from([("a".to_owned(), 1), ("b".to_owned(), 2)]);
    assert_eq!(
        maps::round_trip_simple_map(simple.clone())
            .await
            .expect("map"),
        simple
    );
    let resumes = Map::from([(
        "ada".to_owned(),
        maps::Resume {
            name: "Ada".to_owned(),
        },
    )]);
    assert_eq!(
        maps::round_trip_enum_keyed_map(resumes.clone())
            .await
            .expect("resumes"),
        resumes
    );
    let lists_of = Map::from([("xs".to_owned(), vec![1, 2]), ("empty".to_owned(), vec![])]);
    assert_eq!(
        maps::round_trip_list_valued_map(lists_of.clone())
            .await
            .expect("lists"),
        lists_of
    );
    let maps_container = maps::MapContainer {
        simple,
        enum_keyed: resumes,
        list_valued: lists_of,
    };
    assert_eq!(
        maps::round_trip_map_container(&maps_container)
            .await
            .expect("maps"),
        maps_container
    );

    for value in [None, Some(0), Some(i64::MIN)] {
        assert_eq!(
            optional::round_trip_optional_int(value)
                .await
                .expect("int?"),
            value
        );
    }
    let resume = Some(optional::Resume {
        name: "Grace".to_owned(),
    });
    assert_eq!(
        optional::round_trip_optional_resume(resume.clone())
            .await
            .expect("resume?"),
        resume
    );
    assert_eq!(
        optional::round_trip_optional_union(None)
            .await
            .expect("union?"),
        None
    );
    assert_eq!(
        optional::round_trip_optional_union(optional::IntOrString::String("s".to_owned()))
            .await
            .expect("union?"),
        Some(optional::IntOrString::String("s".to_owned()))
    );
    let optional_container = optional::OptionalContainer {
        optional_int: None,
        optional_class: resume,
        optional_union: Some(optional::IntOrString::Int(5)),
    };
    assert_eq!(
        optional::round_trip_optional_container(&optional_container)
            .await
            .expect("optionals"),
        optional_container
    );

    assert_eq!(
        unions::round_trip_null_to_end(unions::IntOrString::Int(3))
            .await
            .expect("union"),
        Some(unions::IntOrString::Int(3))
    );
    let union_container = unions::UnionContainer {
        null_to_end: None,
        dedup: unions::IntOrString::String("d".to_owned()),
        singleton_unwrap: 1,
        optional_plus_null: Some(unions::TOrString::T(unions::T { v: 2 })),
    };
    assert_eq!(
        unions::round_trip_union_container(&union_container)
            .await
            .expect("unions"),
        union_container
    );
    for value in [
        unions::StringListOrIntList::IntList(vec![1, 2]),
        unions::StringListOrIntList::StringList(vec!["a".to_owned()]),
    ] {
        assert_eq!(
            unions::round_trip_str_or_int_list(value.clone())
                .await
                .expect("lists"),
            value
        );
    }
    assert_eq!(
        unions::round_trip_optional_plus_null(unions::TOrString::String("t".to_owned()))
            .await
            .expect("t or string"),
        Some(unions::TOrString::String("t".to_owned()))
    );

    assert_eq!(
        primitives::round_trip_int(i64::MAX).await.expect("int"),
        i64::MAX
    );
    assert_eq!(
        primitives::round_trip_float(-0.5).await.expect("float"),
        -0.5
    );
    assert_eq!(
        primitives::round_trip_string("héllo, wörld".to_owned())
            .await
            .expect("string"),
        "héllo, wörld"
    );
    assert!(primitives::round_trip_bool(true).await.expect("bool"));
    primitives::return_null().await.expect("null");
}

#[test]
fn every_round_trip_returns_what_went_in() {
    let (_turn, library) = library();
    let before = library.handle_count();
    runtime().block_on(every_round_trip());
    assert_eq!(library.handle_count(), before, "every handle is freed");
}

/// The same, in a process of its own that collects on every allocation.
#[test]
fn every_round_trip_returns_what_went_in_collecting_always() {
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "--exact",
            "every_round_trip_returns_what_went_in",
            "--nocapture",
        ])
        .env("BAML_GC_STRESS", "1")
        .env("BAML_NATIVE_TEST_LIBRARY", library_path())
        .output()
        .expect("the test binary runs");
    assert!(
        output.status.success(),
        "under BAML_GC_STRESS:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A function that throws a class: the error is typed.
#[test]
fn a_thrown_class_arrives_typed() {
    let (_turn, library) = library();
    let before = library.handle_count();
    runtime().block_on(async {
        assert_eq!(
            bridge_test::lookup("here".to_owned()).await.expect("found"),
            "here"
        );
        let error = bridge_test::lookup("missing".to_owned())
            .await
            .expect_err("missing");
        assert_eq!(
            error,
            Error::Thrown {
                value: Box::new(bridge_test::LookupFailed {
                    key: "missing".to_owned(),
                    code: 404,
                }),
            }
        );
    });
    assert_eq!(library.handle_count(), before);
}

/// Bindings for another version of the program are refused at load.
#[test]
fn a_library_with_another_interface_is_refused() {
    let (_turn, _library) = library();
    let path = library_path().to_str().expect("UTF-8");
    match Library::load(path, bindings::INTERFACE_HASH ^ 1) {
        Err(SdkError::Load(why)) => assert!(why.contains("interface hash"), "{why}"),
        Ok(_) => panic!("a mismatched hash is accepted"),
        Err(other) => panic!("{other}"),
    }
}

/// Ten thousand instances in and out: the handle table grows.
#[test]
fn ten_thousand_instances_round_trip() {
    let (_turn, library) = library();
    let before = library.handle_count();
    let points: Vec<bridge_test::Point> = (0..10_000)
        .map(|i| bridge_test::Point { x: i, y: -i })
        .collect();
    let back = runtime()
        .block_on(bridge_test::round_trip_points(points.clone()))
        .expect("points");
    assert_eq!(back, points);
    assert_eq!(library.handle_count(), before);
}

/// Calls that park and resume, on a runtime with several threads, with
/// collections forced while they are parked: each finishes with what it
/// should, whichever thread polls it.
#[test]
fn calls_park_and_resume_across_threads() {
    let (_turn, library) = library();
    let before = library.handle_count();
    runtime().block_on(async {
        let mut tasks = Vec::new();
        for i in 0..16_i64 {
            tasks.push(tokio::spawn(async move {
                let slept = bridge_test::nap(1 + i % 4).await.expect("nap");
                let home = bridge_test::env_or("HOME".to_owned(), "?".to_owned())
                    .await
                    .expect("env");
                let profile = profile::complex_profile();
                let back = complex_models::round_trip_complex_profile(&profile)
                    .await
                    .expect("profile");
                (slept, home, back == profile)
            }));
        }
        let collector = tokio::spawn(async move {
            for _ in 0..20 {
                library.collect();
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        for (i, task) in tasks.into_iter().enumerate() {
            let (slept, home, same) = task.await.expect("the task ends");
            assert_eq!(slept, 1 + i64::try_from(i).expect("small") % 4);
            assert_eq!(home, std::env::var("HOME").expect("HOME"));
            assert!(same);
        }
        collector.await.expect("the collector ends");
    });
    assert_eq!(library.handle_count(), before);
}

/// A call dropped while it waits (a timeout) does not stop the others,
/// and its result is freed when it arrives.
#[test]
fn a_dropped_call_loses_no_wake() {
    let (_turn, library) = library();
    let before = library.handle_count();
    runtime().block_on(async {
        let long = bridge_test::nap(30);
        let short = tokio::time::timeout(
            Duration::from_millis(5),
            bridge_test::env_or("HOME".to_owned(), "?".to_owned()),
        );
        let dropped = tokio::time::timeout(Duration::from_millis(1), bridge_test::nap(20));
        let (long, short, dropped) = tokio::join!(long, short, dropped);
        assert_eq!(long.expect("the long nap"), 30);
        assert!(short.is_ok(), "the short call finished in time");
        assert!(dropped.is_err(), "the dropped call timed out");
        // The dropped call's task still ends; then nothing is held.
        tokio::time::sleep(Duration::from_millis(40)).await;
        bridge_test::add(1, 1).await.expect("add");
    });
    assert_eq!(library.handle_count(), before);
}

/// A value of the wrong type is refused where it lands, and an array lands
/// in one place only.
#[test]
fn checked_writes_and_one_use_containers() {
    let (_turn, library) = library();
    let session = library.session();
    let wrong = session.instance(
        1,
        vec![session.string("x").expect("a string"), session.int(1)],
    );
    match wrong {
        Err(SdkError::Refused(why)) => assert!(why.contains("field 0"), "{why}"),
        other => panic!(
            "a string field where an int is declared: {:?}",
            other.map(|v| v.raw())
        ),
    }
    let ints = session
        .array(vec![session.int(1), session.int(2)])
        .expect("an array");
    let strings = session.array(vec![]).expect("an array");
    let unions = session.array(vec![]).expect("an array");
    let first = session
        .instance(18, vec![ints, strings, unions])
        .expect("a list container");
    // The array inside `first` cannot be passed again: a field read is no
    // fresh array.
    let again = first.field(0).expect("its ints");
    let refused = session.instance(
        18,
        vec![
            again,
            session.array(vec![]).expect("an array"),
            session.array(vec![]).expect("an array"),
        ],
    );
    match refused {
        Err(SdkError::Refused(why)) => assert!(why.contains("passed once"), "{why}"),
        other => panic!("an array passed twice: {:?}", other.map(|v| v.raw())),
    }
}
