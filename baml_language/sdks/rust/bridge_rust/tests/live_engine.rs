//! End-to-end tests of the `baml_bridge` call path against a live engine:
//! inline BAML source → compile → invoke → decode, exercising the same
//! `bridge_cffi` / `bridge_ctypes` machinery a generated SDK composes.

use std::{collections::HashMap, convert::Infallible, sync::OnceLock};

mod common;

use baml_bridge::{BamlValue, BigInt, Error, OptionalArg, encode, runtime};

const BAML_SRC: &str = r#"
function rt_int(x: int) -> int { x }
function rt_float(x: float) -> float { x }
function rt_string(x: string) -> string { x }
function rt_bool(x: bool) -> bool { x }
function rt_bigint(x: bigint) -> bigint { x }
function rt_bytes(x: uint8array) -> uint8array { x }
function rt_opt(x: int?) -> int? { x }
function rt_list(x: int[]) -> int[] { x }
function no_op() -> void {}
function opt_probe(a: int, o: int? = 5) -> int?[] { [a, o] }
"#;

/// Initialize the process-global runtime once for every test in this
/// binary (they share the singleton by design).
fn ensure_runtime() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        common::locate_dev_engine();
        let files = HashMap::from([("main.baml".to_string(), BAML_SRC.to_string())]);
        // The vfs rejects trailing slashes (macOS's temp_dir ends in one).
        let root = std::env::temp_dir();
        let root = root.to_string_lossy();
        let root = root.trim_end_matches('/');
        runtime::initialize_from_files(root, &files).expect("runtime initializes");
    });
}

/// Call a unary echo function synchronously and decode as the same type.
fn round_trip<T: BamlValue>(fqn: &str, value: &T) -> T {
    ensure_runtime();
    runtime::invoke_sync::<T, Infallible>(fqn, encode::kwargs(vec![("x", Some(value.to_baml()))]))
        .unwrap_or_else(|e| panic!("{fqn} failed: {e}"))
}

trait ToBamlForTest {
    fn to_baml(&self) -> baml_bridge::wire::InboundValue;
}

impl<T: BamlValue> ToBamlForTest for T {
    fn to_baml(&self) -> baml_bridge::wire::InboundValue {
        baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(self)
    }
}

#[test]
fn round_trips_int() {
    assert_eq!(round_trip("user.rt_int", &7i64), 7);
}

#[test]
#[expect(clippy::float_cmp, reason = "the round trip must be bit-exact")]
fn round_trips_float() {
    assert_eq!(round_trip("user.rt_float", &2.5f64), 2.5);
}

#[test]
fn round_trips_string() {
    assert_eq!(round_trip("user.rt_string", &"hi 🐑".to_string()), "hi 🐑");
}

#[test]
fn round_trips_bool() {
    assert!(!round_trip("user.rt_bool", &false));
}

#[test]
fn round_trips_bigint() {
    let big: BigInt = BigInt::from(-1_234_567_890i64) * BigInt::from(987_654_321i64);
    assert_eq!(round_trip("user.rt_bigint", &big), big);
}

#[test]
fn round_trips_bytes() {
    let bytes = b"\x00\x01\xff".to_vec();
    assert_eq!(round_trip("user.rt_bytes", &bytes), bytes);
}

#[test]
fn round_trips_optional_some_and_none() {
    assert_eq!(round_trip("user.rt_opt", &Some(3i64)), Some(3));
    assert_eq!(round_trip("user.rt_opt", &None::<i64>), None);
}

#[test]
fn round_trips_list() {
    let list = vec![1i64, 2, 3];
    assert_eq!(round_trip("user.rt_list", &list), list);
}

#[test]
fn void_returns_unit() {
    ensure_runtime();
    runtime::invoke_sync::<(), Infallible>("user.no_op", encode::kwargs(vec![]))
        .expect("no_op succeeds");
}

#[test]
fn unset_takes_default_and_null_stays_null() {
    ensure_runtime();
    let call = |o: OptionalArg<Option<i64>>| {
        runtime::invoke_sync::<Vec<Option<i64>>, Infallible>(
            "user.opt_probe",
            encode::kwargs(vec![("a", Some(1i64.to_baml())), ("o", o.to_baml_opt())]),
        )
        .expect("opt_probe succeeds")
    };
    assert_eq!(call(OptionalArg::Unset), [Some(1), Some(5)]);
    assert_eq!(call(OptionalArg::Set(None)), [Some(1), None]);
    assert_eq!(call(OptionalArg::Set(Some(9))), [Some(1), Some(9)]);
}

#[test]
fn unknown_function_is_a_panic() {
    // The engine reports a missing entry point on the panic arm (the same
    // envelope python surfaces as `BamlPanic`), not as a thrown error.
    ensure_runtime();
    let result = runtime::invoke_sync::<i64, Infallible>("user.does_not_exist", vec![]);
    match result {
        Err(Error::Panic { message, .. }) => {
            assert!(
                message.contains("user.does_not_exist"),
                "message: {message}"
            );
        }
        other => panic!("expected a panic, got {other:?}"),
    }
}

#[tokio::test]
async fn async_invoke_round_trips() {
    ensure_runtime();
    let result: i64 = runtime::invoke::<i64, Infallible>(
        "user.rt_int",
        encode::kwargs(vec![("x", Some(42i64.to_baml()))]),
    )
    .await
    .expect("async rt_int succeeds");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn sync_invoke_inside_async_runtime_is_refused() {
    ensure_runtime();
    let result = runtime::invoke_sync::<i64, Infallible>(
        "user.rt_int",
        encode::kwargs(vec![("x", Some(1i64.to_baml()))]),
    );
    assert!(matches!(result, Err(Error::CalledSyncFromAsync)));
}
