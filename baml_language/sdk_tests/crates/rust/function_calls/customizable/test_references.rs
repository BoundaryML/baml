//! Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
//! references: one passed back is the same instance, wherever a class can
//! sit. Its state shows which instance a call reached: one hasher fed "a" and
//! then "b" hashes as a fresh one fed "ab".

use std::sync::{Arc, Mutex};

use baml_sdk::baml::hash::DefaultHasher;
use baml_sdk::references::{
    DefaultHasherOrHasherMarker, HasherMarker, describe_hasher_or_marker_sync, hash_after_sync,
    hasher_or_marker_sync, hasher_through_callback_sync, hasher_twice_sync, hasher_under_sync,
    hold_hasher_sync, make_hasher_sync, maybe_hasher_sync, same_hasher_sync,
};

#[test]
fn test_references_a_returned_instance_passed_back_is_the_same_instance() {
    let hasher = make_hasher_sync().unwrap();
    hash_after_sync(&hasher, "a".to_string()).unwrap();
    let fed_twice = hash_after_sync(&hasher, "b".to_string()).unwrap();
    assert_eq!(
        fed_twice,
        hash_after_sync(make_hasher_sync().unwrap(), "ab".to_string()).unwrap()
    );
    // Negative control: a fresh hasher fed only "b" differs.
    assert_ne!(
        fed_twice,
        hash_after_sync(make_hasher_sync().unwrap(), "b".to_string()).unwrap()
    );
}

#[test]
fn test_references_one_engine_instance_is_one_reference() {
    let hasher = make_hasher_sync().unwrap();
    assert_eq!(same_hasher_sync(&hasher).unwrap(), hasher);
    assert_ne!(make_hasher_sync().unwrap(), hasher);
}

#[test]
fn test_references_containers_hold_the_instance() {
    let hasher = make_hasher_sync().unwrap();
    assert!(maybe_hasher_sync(true).unwrap().is_some());
    assert!(maybe_hasher_sync(false).unwrap().is_none());
    assert_eq!(
        hasher_twice_sync(&hasher).unwrap(),
        vec![hasher.clone(), hasher.clone()]
    );
    let under = hasher_under_sync("key".to_string(), &hasher).unwrap();
    assert_eq!(under.len(), 1);
    assert_eq!(under.get("key"), Some(&hasher));
}

#[test]
fn test_references_a_field_holds_the_instance() {
    let hasher = make_hasher_sync().unwrap();
    let holder = hold_hasher_sync("label".to_string(), &hasher).unwrap();
    assert_eq!(holder.label, "label");
    assert_eq!(holder.hasher, hasher);
}

#[test]
fn test_references_a_union_selects_the_instance_arm() {
    assert!(matches!(
        hasher_or_marker_sync(true).unwrap(),
        DefaultHasherOrHasherMarker::DefaultHasher(_)
    ));
    let marker = HasherMarker {
        tag: "marker".to_string(),
    };
    assert_eq!(
        hasher_or_marker_sync(false).unwrap(),
        DefaultHasherOrHasherMarker::HasherMarker(marker.clone())
    );
    assert_eq!(
        describe_hasher_or_marker_sync(DefaultHasherOrHasherMarker::DefaultHasher(
            make_hasher_sync().unwrap()
        ))
        .unwrap(),
        "hasher"
    );
    assert_eq!(
        describe_hasher_or_marker_sync(DefaultHasherOrHasherMarker::HasherMarker(marker)).unwrap(),
        "marker"
    );
}

#[test]
fn test_references_a_callback_receives_and_returns_the_instance() {
    let hasher = make_hasher_sync().unwrap();
    let received: Arc<Mutex<Vec<DefaultHasher>>> = Arc::default();
    let record = Arc::clone(&received);
    let callback = move |value: DefaultHasher| {
        record.lock().unwrap().push(value.clone());
        value
    };
    assert_eq!(
        hasher_through_callback_sync(&hasher, callback).unwrap(),
        hasher
    );
    assert_eq!(*received.lock().unwrap(), vec![hasher]);
}
