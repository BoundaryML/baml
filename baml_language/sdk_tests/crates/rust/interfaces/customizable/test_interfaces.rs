//! Owned BAML receivers, defaults, and copied records with live children.
use baml_sdk::*;

#[tokio::test]
async fn test_baml_receiver_roundtrip() {
    let greeter = make_greeter_async("Hello".into()).await.unwrap();
    let returned = pass_greeter_async(&greeter).await.unwrap();
    assert_eq!(greeter.greet("Ada".into()).await.unwrap(), "Hello, Ada!");
    assert_eq!(
        returned.greet("Grace".into()).await.unwrap(),
        "Hello, Grace!"
    );
    assert_eq!(
        welcome_async(&returned, "Lin".into()).await.unwrap(),
        "Hello, Lin!"
    );
}

#[tokio::test]
async fn test_concrete_implements_interface_input() {
    let greeter = FriendlyGreeter::new("Hello".into()).await.unwrap();
    assert_eq!(
        welcome_async(&greeter, "Ada".into()).await.unwrap(),
        "Hello, Ada!"
    );
}

#[tokio::test]
async fn test_default_method_dispatch() {
    let greeter = make_greeter_async("Hello".into()).await.unwrap();
    assert_eq!(greeter.label().await.unwrap(), "greeter");
    assert_eq!(greeter_label_async(&greeter).await.unwrap(), "greeter");
}

#[tokio::test]
async fn test_owner_state_survives_method_calls() {
    let counter = make_counter_async(10).await.unwrap();
    let returned = pass_counter_async(&counter).await.unwrap();
    assert_eq!(counter.add(2).await.unwrap(), 12);
    assert_eq!(add_in_baml_async(&returned, 5).await.unwrap(), 17);
    assert_eq!(counter.current().await.unwrap(), 17);
    assert_eq!(returned.current().await.unwrap(), 17);
}

#[tokio::test]
async fn test_record_copy_preserves_live_child() {
    let counter = make_counter_async(10).await.unwrap();
    let mut record = counter_record_async(&counter).await.unwrap();
    record.title = "local edit".into();
    assert_eq!(record.counter.add(2).await.unwrap(), 12);
    assert_eq!(counter.current().await.unwrap(), 12);
    let fresh = counter_record_async(&counter).await.unwrap();
    assert_eq!(fresh.title, "counter");
    assert_eq!(fresh.counter.current().await.unwrap(), 12);
}

#[tokio::test]
async fn test_copied_record_implements_interface_input() {
    let mut record = marker_record_async("initial".into()).await.unwrap();
    let retained = pass_marker_async(&record).await.unwrap();
    record.text = "local edit".into();
    assert_eq!(marker_text_async(&retained).await.unwrap(), "initial");
    assert_eq!(marker_text_async(&record).await.unwrap(), "local edit");
}

#[tokio::test]
async fn test_inherited_method_preserves_receiver_state() {
    let counter = make_extended_counter_async(10).await.unwrap();
    assert_eq!(counter.add(2).await.unwrap(), 12);
    let parent = pass_counter_async(&counter).await.unwrap();
    assert_eq!(parent.add(3).await.unwrap(), 15);
    assert_eq!(counter.current().await.unwrap(), 15);
    drop(counter);
    assert_eq!(parent.current().await.unwrap(), 15);
}
