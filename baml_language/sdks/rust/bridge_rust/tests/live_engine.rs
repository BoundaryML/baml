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
function gid<T>(x: T) -> T { x }
function gname<T>() -> string { reflect.Type.of<T>().to_string() }
function rt_wide_literals(
  x: (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12)
) -> (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12) { x }
function make_unknown_pair() -> (int, unknown) { (1, "a") }
function unknown_pair_second(x: (int, unknown)) -> unknown { x[1] }
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
    runtime::invoke_sync::<T, Infallible>(fqn, |args| {
        args.value("x", value, &ty_of::<T>())?;
        Ok(vec![])
    })
    .unwrap_or_else(|e| panic!("{fqn} failed: {e}"))
}

/// The wire `BamlTy` a generated SDK would send for a `TypeVar` bound to
/// `T` (generated code reaches it via the same private trait).
fn ty_of<T: BamlValue>() -> baml_bridge::wire::BamlTy {
    <T as baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty()
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
    runtime::invoke_sync::<(), Infallible>("user.no_op", |_| Ok(vec![])).expect("no_op succeeds");
}

#[test]
fn unset_takes_default_and_null_stays_null() {
    ensure_runtime();
    let call = |o: OptionalArg<Option<i64>>| {
        runtime::invoke_sync::<Vec<Option<i64>>, Infallible>("user.opt_probe", |args| {
            args.value("a", &1i64, &ty_of::<i64>())?;
            args.optional("o", &o, &ty_of::<Option<i64>>())?;
            Ok(vec![])
        })
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
    let result = runtime::invoke_sync::<i64, Infallible>("user.does_not_exist", |_| Ok(vec![]));
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
    let result: i64 = runtime::invoke::<i64, Infallible>("user.rt_int", |args| {
        args.value("x", &42i64, &ty_of::<i64>())?;
        Ok(vec![])
    })
    .await
    .expect("async rt_int succeeds");
    assert_eq!(result, 42);
}

#[tokio::test]
async fn sync_invoke_inside_async_runtime_is_refused() {
    ensure_runtime();
    let result = runtime::invoke_sync::<i64, Infallible>("user.rt_int", |args| {
        args.value("x", &1i64, &ty_of::<i64>())?;
        Ok(vec![])
    });
    assert!(matches!(result, Err(Error::CalledSyncFromAsync)));
}

#[test]
fn generic_identity_round_trips_with_explicit_type_args() {
    ensure_runtime();
    // gid<T>(x: T) -> T, bound explicitly (as generated code always does).
    let seven = runtime::invoke_sync::<i64, Infallible>("user.gid", |args| {
        args.value("x", &7i64, &ty_of::<i64>())?;
        Ok(encode::type_args(vec![("T", ty_of::<i64>())]))
    })
    .expect("gid<int> succeeds");
    assert_eq!(seven, 7);

    let hi = runtime::invoke_sync::<String, Infallible>("user.gid", |args| {
        args.value("x", &"hi".to_string(), &ty_of::<String>())?;
        Ok(encode::type_args(vec![("T", ty_of::<String>())]))
    })
    .expect("gid<string> succeeds");
    assert_eq!(hi, "hi");

    // A container type argument exercises the nested `BamlTy` construction
    // (`list<int>`) on the wire, round-tripping the whole value back.
    let xs = vec![1i64, 2, 3];
    let out = runtime::invoke_sync::<Vec<i64>, Infallible>("user.gid", |args| {
        args.value("x", &xs, &ty_of::<Vec<i64>>())?;
        Ok(encode::type_args(vec![("T", ty_of::<Vec<i64>>())]))
    })
    .expect("gid<int[]> succeeds");
    assert_eq!(out, xs);
}

#[test]
fn return_only_type_param_is_bound_from_type_args() {
    ensure_runtime();
    // gname<T>() has no argument carrying `T`, so the binding can only come
    // from the explicit type_args — proving they are honored, not merely
    // recovered from argument inference. `reflect.Type.of<T>()` reflects the
    // bound type's name back as a string.
    let int_name = runtime::invoke_sync::<String, Infallible>("user.gname", |_| {
        Ok(encode::type_args(vec![("T", ty_of::<i64>())]))
    })
    .expect("gname<int> succeeds");
    assert_eq!(int_name, "int");

    let string_name = runtime::invoke_sync::<String, Infallible>("user.gname", |_| {
        Ok(encode::type_args(vec![("T", ty_of::<String>())]))
    })
    .expect("gname<string> succeeds");
    assert_eq!(string_name, "string");
}

/// The declared type of `rt_wide_literals`' parameter, as a generated
/// binding states it: a tuple of the int literals `0` through `12`.
fn wide_literals_ty() -> baml_bridge::wire::BamlTy {
    use baml_bridge::{baml_value::internal, wire};
    internal::tuple_ty(
        (0..13)
            .map(|value| internal::literal_ty(wire::baml_ty_literal::Literal::IntValue(value)))
            .collect(),
    )
}

fn call_wide_literals(xs: &Vec<i64>) -> Result<Vec<i64>, Error<Infallible>> {
    ensure_runtime();
    runtime::invoke_sync::<Vec<i64>, Infallible>("user.rt_wide_literals", |args| {
        args.value("x", xs, &wide_literals_ty())?;
        Ok(vec![])
    })
}

#[test]
fn a_list_standing_in_for_a_tuple_round_trips_as_the_declared_tuple() {
    // A tuple with no Rust tuple or named struct (its literal slots have no
    // name token) is a list of its slots' join. The list states the declared
    // tuple type going in, and the tuple decodes as that list coming back.
    let xs: Vec<i64> = (0..13).collect();
    assert_eq!(call_wide_literals(&xs).expect("the tuple round-trips"), xs);
}

#[test]
fn a_list_standing_in_for_a_tuple_is_checked_slot_by_slot_and_by_arity() {
    // The engine checks each element against its own literal slot...
    let mut wrong_slot: Vec<i64> = (0..13).collect();
    wrong_slot[12] = 0;
    assert!(call_wide_literals(&wrong_slot).is_err());
    // ...and the arity of the declared tuple.
    let short: Vec<i64> = (0..12).collect();
    assert!(call_wide_literals(&short).is_err());
}

#[test]
fn a_list_of_dynamic_values_stands_in_for_a_tuple_with_an_unknown_slot() {
    ensure_runtime();
    // `(int, unknown)` has no Rust tuple, and `unknown` no union arm: the
    // tuple is a list of dynamic values, each sent back as it arrived.
    let pair = runtime::invoke_sync::<Vec<baml_bridge::dynamic::Value>, Infallible>(
        "user.make_unknown_pair",
        |_| Ok(vec![]),
    )
    .expect("make_unknown_pair succeeds");
    assert_eq!(pair.len(), 2);
    assert_eq!(pair[0].decode::<i64>().unwrap(), 1);
    let declared = {
        use baml_bridge::{baml_value::internal, wire};
        internal::tuple_ty(vec![
            internal::primitive_ty(wire::BamlTyPrimitiveKind::BamlTyPrimitiveInt),
            internal::leaf_ty(wire::baml_ty::Ty::Unknown(wire::BamlTyUnknown {})),
        ])
    };
    let second = runtime::invoke_sync::<baml_bridge::dynamic::Value, Infallible>(
        "user.unknown_pair_second",
        |args| {
            args.value("x", &pair, &declared)?;
            Ok(vec![])
        },
    )
    .expect("unknown_pair_second succeeds");
    assert_eq!(second.decode::<String>().unwrap(), "a");
}
