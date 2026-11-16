//! `trace::tracing_layer` through the public generated SDK. The subscriber is
//! scoped to each test, so a test's spans never reach another's.
use baml_sdk::host_callable_tests as baml;
use baml_sdk::{hello_world, hello_world_sync, trace};
use tracing::instrument::WithSubscriber;
use tracing::{Instrument, info_span, instrument};
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, Registry};

fn subscriber() -> impl tracing::Subscriber + Send + Sync {
    Registry::default().with(trace::tracing_layer().unwrap())
}

/// What BAML sees as the current identity through a real engine call.
fn distinct_id_in_a_baml_call() -> Option<String> {
    baml::call_with_callback_sync(
        |_: i64| -> String {
            trace::current_context_sync()
                .unwrap()
                .distinct_id
                .unwrap_or_default()
        },
        1,
    )
    .ok()
    .filter(|id| !id.is_empty())
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[test]
fn test_tracing_layer_baml_calls_nest_under_the_entered_span() {
    tracing::subscriber::with_default(subscriber(), || {
        assert_eq!(distinct_id_in_a_baml_call(), None);
        let span = info_span!("load_batch", baml.distinct_id = "entered", rows = 3);
        {
            let _entered = span.enter();
            hello_world_sync().unwrap();
            assert_eq!(distinct_id_in_a_baml_call().as_deref(), Some("entered"));
        }
        assert_eq!(distinct_id_in_a_baml_call(), None);
    });
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[test]
fn test_tracing_layer_child_span_inherits_its_parents_context() {
    tracing::subscriber::with_default(subscriber(), || {
        let parent = info_span!("parent", baml.distinct_id = "parent");
        let child = info_span!(parent: &parent, "child");
        let _entered = child.enter();
        assert_eq!(distinct_id_in_a_baml_call().as_deref(), Some("parent"));
    });
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[test]
fn test_tracing_layer_filtered_parent_is_skipped_over() {
    let layer = trace::tracing_layer()
        .unwrap()
        .with_filter(filter_fn(|metadata| metadata.name() != "hidden"));
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let kept = info_span!("kept", baml.distinct_id = "kept");
        let _kept = kept.enter();
        let hidden = info_span!("hidden", baml.distinct_id = "hidden");
        let _hidden = hidden.enter();
        let inner = info_span!("inner");
        let _inner = inner.enter();
        assert_eq!(distinct_id_in_a_baml_call().as_deref(), Some("kept"));
    });
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[test]
fn test_tracing_layer_explicit_root_starts_a_new_trace() {
    tracing::subscriber::with_default(subscriber(), || {
        let outer = info_span!("outer", baml.distinct_id = "outer");
        let _outer = outer.enter();
        let root = info_span!(parent: None, "root");
        let _root = root.enter();
        assert_eq!(distinct_id_in_a_baml_call(), None);
    });
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[test]
fn test_tracing_layer_callback_runs_under_its_frame_not_an_older_span() {
    let dispatch = tracing::Dispatch::new(subscriber());
    tracing::dispatcher::with_default(&dispatch.clone(), || {
        let outer = info_span!("outer", baml.distinct_id = "outer");
        let _outer = outer.enter();
        let inner = baml::call_with_callback_sync(
            move |_: i64| -> String {
                tracing::dispatcher::with_default(&dispatch, || {
                    assert_eq!(
                        trace::current_context_sync()
                            .unwrap()
                            .distinct_id
                            .as_deref(),
                        Some("outer")
                    );
                    let span = info_span!("in_callback", baml.distinct_id = "in_callback");
                    let _entered = span.enter();
                    distinct_id_in_a_baml_call().unwrap_or_default()
                })
            },
            1,
        )
        .unwrap();
        assert_eq!(inner, "in_callback");
        assert_eq!(distinct_id_in_a_baml_call().as_deref(), Some("outer"));
    });
}

#[instrument(fields(baml.distinct_id = "async", baml.metadata.stage = "load"), ret, err)]
async fn instrumented(page: i64) -> Result<i64, String> {
    tokio::task::yield_now().await;
    hello_world().await.map_err(|error| error.to_string())?;
    if page < 0 {
        return Err("negative page".to_string());
    }
    let context = trace::current_context().await.unwrap();
    assert_eq!(context.distinct_id.as_deref(), Some("async"));
    assert!(context.metadata.contains_key("stage"));
    Ok(page)
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[tokio::test]
async fn test_tracing_layer_instrument_stays_open_across_await() {
    let _default = tracing::subscriber::set_default(subscriber());
    assert_eq!(instrumented(7).await, Ok(7));
    assert_eq!(instrumented(-1).await, Err("negative page".to_string()));
    assert!(
        trace::current_context()
            .await
            .unwrap()
            .distinct_id
            .is_none()
    );
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[tokio::test(flavor = "multi_thread")]
async fn test_tracing_layer_instrumented_task_keeps_its_parent() {
    let dispatch = tracing::Dispatch::new(subscriber());
    let _default = tracing::dispatcher::set_default(&dispatch);
    let parent = info_span!("parent", baml.distinct_id = "spawned");
    let task = tokio::spawn(
        async {
            tokio::task::yield_now().await;
            trace::current_context().await.unwrap().distinct_id
        }
        .instrument(parent)
        // A scoped default stays on this thread; a global subscriber needs no this.
        .with_subscriber(dispatch.clone()),
    );
    assert_eq!(task.await.unwrap().as_deref(), Some("spawned"));
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[test]
fn test_tracing_layer_late_fields_ride_a_reported_return() {
    #[instrument(fields(rows = tracing::field::Empty), ret)]
    fn late_with_return() -> i64 {
        tracing::Span::current().record("rows", 3);
        7
    }
    tracing::subscriber::with_default(subscriber(), || {
        assert_eq!(late_with_return(), 7);
        let dropped = info_span!("late_without_return", rows = tracing::field::Empty);
        dropped.record("rows", 3);
    });
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[test]
fn test_tracing_layer_panic_reaches_the_span() {
    tracing::subscriber::with_default(subscriber(), || {
        let unwound = std::panic::catch_unwind(|| {
            let span = info_span!("panicking");
            let _entered = span.enter();
            panic!("expected");
        });
        assert!(unwound.is_err());
    });
}

// SDK_PARITY_LINT(skip): the `tracing` crate exists only in Rust
#[test]
fn test_tracing_layer_open_span_outlives_the_test() {
    let dispatch = tracing::Dispatch::new(subscriber());
    tracing::dispatcher::with_default(&dispatch, || {
        std::mem::forget(info_span!("never_closed"));
    });
    // Leaked with its subscriber: process-exit shutdown must not wait for it.
    std::mem::forget(dispatch);
}
