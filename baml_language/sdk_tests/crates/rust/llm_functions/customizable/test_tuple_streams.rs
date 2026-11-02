//! Tuples as stream items, offline: `baml_sdk::tuple_streams`' functions
//! stream through a scripted `StreamingClient` (fixed text deltas, no
//! replay server or key), so each partial and the final value decode
//! through the ordinary stream companion.
//!
//!   * `stream_tuple_items` -- `(string, int)[]`: every partial is a list of
//!     native Rust tuples, the first `[("a", 1)]`.
//!   * `stream_tuple_final` -- `(int, string)`: every partial is itself a
//!     tuple, and so is the final value.
use baml_sdk::tuple_streams::{
    stream_tuple_final, stream_tuple_final_stream, stream_tuple_final_stream_async,
    stream_tuple_items, stream_tuple_items_stream, stream_tuple_items_stream_async,
};

fn s(value: &str) -> String {
    value.to_string()
}

fn final_items() -> Vec<(String, i64)> {
    vec![(s("a"), 1), (s("b"), 2)]
}

/// Each partial is a prefix of the final list, starting from one complete
/// tuple.
fn check_item_partials(partials: &[Vec<(String, i64)>]) {
    assert!(!partials.is_empty(), "expected at least one partial");
    assert_eq!(partials[0], vec![(s("a"), 1)]);
    for partial in partials {
        assert!(final_items().starts_with(partial), "{partial:?}");
    }
}

/// The async stream companion, driven to completion on its own runtime.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

// SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
#[test]
fn test_tuples_stream_items() {
    let stream = stream_tuple_items_stream(s("x")).unwrap();
    let mut partials = Vec::new();
    while let Some(partial) = stream.next().unwrap() {
        partials.push(partial);
    }
    check_item_partials(&partials);
    assert_eq!(stream.final_().unwrap(), final_items());

    let (partials, last) = block_on(async {
        let stream = stream_tuple_items_stream_async(s("x")).await.unwrap();
        let mut partials = Vec::new();
        while let Some(partial) = stream.next_async().await.unwrap() {
            partials.push(partial);
        }
        (partials, stream.final_async().await.unwrap())
    });
    check_item_partials(&partials);
    assert_eq!(last, final_items());

    // The same function called outright.
    assert_eq!(stream_tuple_items(s("x")).unwrap(), final_items());
}

// SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
#[test]
fn test_tuples_stream_final() {
    let stream = stream_tuple_final_stream(s("x")).unwrap();
    let mut partials: Vec<(i64, String)> = Vec::new();
    while let Some(partial) = stream.next().unwrap() {
        partials.push(partial);
    }
    // A partial tuple has every slot: the string slot fills in as it streams.
    assert!(!partials.is_empty(), "expected at least one partial");
    assert!(
        partials
            .iter()
            .all(|(n, tag)| *n == 1 && "a".starts_with(tag.as_str())),
        "{partials:?}"
    );
    assert_eq!(partials.last(), Some(&(1, s("a"))));
    assert_eq!(stream.final_().unwrap(), (1, s("a")));

    let (partials, last) = block_on(async {
        let stream = stream_tuple_final_stream_async(s("x")).await.unwrap();
        let mut partials: Vec<(i64, String)> = Vec::new();
        while let Some(partial) = stream.next_async().await.unwrap() {
            partials.push(partial);
        }
        (partials, stream.final_async().await.unwrap())
    });
    assert_eq!(partials.last(), Some(&(1, s("a"))));
    assert_eq!(last, (1, s("a")));

    assert_eq!(stream_tuple_final(s("x")).unwrap(), (1, s("a")));
}
