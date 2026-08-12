use baml_bridge::OptionalArg::Unset;
use baml_sdk::{OptBox, optional_args_probe, optional_args_probe_async};

#[test]
fn test_optional_args_runtime_matrix() {
    assert_eq!(
        optional_args_probe(1, Unset, Unset).unwrap(),
        [Some(1), Some(5), Some(99)]
    );
    assert_eq!(
        optional_args_probe(1, None, Unset).unwrap(),
        [Some(1), None, Some(99)]
    );
    assert_eq!(
        optional_args_probe(1, Some(8), Unset).unwrap(),
        [Some(1), Some(8), Some(99)]
    );
    assert_eq!(
        optional_args_probe(1, Unset, None).unwrap(),
        [Some(1), Some(5), None]
    );
    assert_eq!(
        optional_args_probe(1, Unset, Some(9)).unwrap(),
        [Some(1), Some(5), Some(9)]
    );
    assert_eq!(
        optional_args_probe(1, None, None).unwrap(),
        [Some(1), None, None]
    );
    assert_eq!(
        optional_args_probe(1, Some(8), Some(9)).unwrap(),
        [Some(1), Some(8), Some(9)]
    );
}

#[test]
fn test_optional_args_python_unset_and_none_differ_in_one_call() {
    // Unset means "omit this argument"; None means "pass an explicit null".
    // The two must stay distinct within a single call.
    assert_eq!(
        optional_args_probe(1, Unset, None).unwrap(),
        [Some(1), Some(5), None]
    );
    assert_eq!(
        optional_args_probe(1, None, Unset).unwrap(),
        [Some(1), None, Some(99)]
    );
}

#[tokio::test]
async fn test_optional_args_async_samples() {
    assert_eq!(
        optional_args_probe_async(1, Unset, Unset).await.unwrap(),
        [Some(1), Some(5), Some(99)]
    );
    assert_eq!(
        optional_args_probe_async(1, None, Unset).await.unwrap(),
        [Some(1), None, Some(99)]
    );
    assert_eq!(
        optional_args_probe_async(1, Unset, Some(9)).await.unwrap(),
        [Some(1), Some(5), Some(9)]
    );
}

#[test]
fn test_optional_args_opt_box_method_matrix() {
    let boxed = OptBox::make(10, Unset).unwrap();
    assert_eq!(boxed.base, 17);

    let boxed2 = OptBox::make(10, Some(0)).unwrap();
    assert_eq!(boxed2.base, 10);
    assert_eq!(
        boxed2.probe(1, Unset).unwrap(),
        [Some(10), Some(1), Some(5)]
    );
    assert_eq!(
        boxed2.probe(1, Some(8)).unwrap(),
        [Some(10), Some(1), Some(8)]
    );
}

#[test]
fn test_optional_args_negative_runtime_cases_reject() {
    // DIVERGENCE(rust): the python cases here — an unknown keyword argument,
    // a call with no arguments, and a duplicated argument — are compile
    // errors under Rust's typed signatures, not runtime failures. The
    // compile-time coverage lives in the optional_args_static.rs
    // compile-fail probes.
}
