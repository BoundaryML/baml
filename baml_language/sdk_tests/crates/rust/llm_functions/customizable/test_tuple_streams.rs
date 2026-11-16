use baml_sdk::tuple_streams::{
    stream_tuple_final_stream, stream_tuple_final_stream_sync, stream_tuple_final_sync,
    stream_tuple_items_stream, stream_tuple_items_stream_sync, stream_tuple_items_sync,
};

fn s(value: &str) -> String {
    value.to_string()
}

fn final_items() -> Vec<(String, i64)> {
    vec![(s("a"), 1), (s("b"), 2)]
}

fn check_item_partials(partials: &[Vec<(String, i64)>]) {
    assert!(!partials.is_empty(), "expected at least one partial");
    assert_eq!(partials[0], vec![(s("a"), 1)]);
    for partial in partials {
        assert!(final_items().starts_with(partial), "{partial:?}");
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

// SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
#[test]
fn test_tuples_stream_items() {
    let stream = stream_tuple_items_stream_sync(s("x")).unwrap();
    let mut partials = Vec::new();
    while let Some(partial) = stream.next_sync().unwrap() {
        partials.push(partial);
    }
    check_item_partials(&partials);
    assert_eq!(stream.final_sync().unwrap(), final_items());

    let (partials, last) = block_on(async {
        let stream = stream_tuple_items_stream(s("x")).await.unwrap();
        let mut partials = Vec::new();
        while let Some(partial) = stream.next().await.unwrap() {
            partials.push(partial);
        }
        (partials, stream.final_().await.unwrap())
    });
    check_item_partials(&partials);
    assert_eq!(last, final_items());

    assert_eq!(stream_tuple_items_sync(s("x")).unwrap(), final_items());
}

// SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
#[test]
fn test_tuples_stream_final() {
    let stream = stream_tuple_final_stream_sync(s("x")).unwrap();
    let mut partials: Vec<(i64, String)> = Vec::new();
    while let Some(partial) = stream.next_sync().unwrap() {
        partials.push(partial);
    }
    assert!(!partials.is_empty(), "expected at least one partial");
    assert!(
        partials
            .iter()
            .all(|(n, tag)| *n == 1 && "a".starts_with(tag.as_str())),
        "{partials:?}"
    );
    assert_eq!(partials.last(), Some(&(1, s("a"))));
    assert_eq!(stream.final_sync().unwrap(), (1, s("a")));

    let (partials, last) = block_on(async {
        let stream = stream_tuple_final_stream(s("x")).await.unwrap();
        let mut partials: Vec<(i64, String)> = Vec::new();
        while let Some(partial) = stream.next().await.unwrap() {
            partials.push(partial);
        }
        (partials, stream.final_().await.unwrap())
    });
    assert_eq!(partials.last(), Some(&(1, s("a"))));
    assert_eq!(last, (1, s("a")));

    assert_eq!(stream_tuple_final_sync(s("x")).unwrap(), (1, s("a")));
}
