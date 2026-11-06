//! End-to-end tests for a host's own value (`baml.host.Value`): the bridge
//! registers it, BAML holds it by key, and it comes back as the same value.

use baml_bridge::HostValue;
use baml_sdk::{
    baml::sys::collect_garbage,
    host_value_tests::{echo_host_value, same_host_value, twice_host_value},
};

#[test]
fn test_host_values_round_trip_as_the_same_value() {
    let sent = HostValue::new(String::from("mine"));
    let back = echo_host_value(sent.clone()).unwrap();
    assert_eq!(
        back.downcast_ref::<String>().map(String::as_str),
        Some("mine")
    );
    assert_eq!(back, sent);
    // Negative control: another host value with equal contents is another
    // value.
    assert_ne!(back, HostValue::new(String::from("mine")));
}

#[test]
fn test_host_values_come_back_inside_a_list() {
    let sent = HostValue::new(7_u32);
    let back = twice_host_value(sent.clone()).unwrap();
    assert_eq!(back.len(), 2);
    assert!(back.iter().all(|value| *value == sent), "{back:?}");
    assert_eq!(back[0].downcast_ref::<u32>(), Some(&7));
}

#[test]
fn test_host_values_sent_again_stay_the_same_value() {
    let sent = HostValue::new(1_u8);
    let first = echo_host_value(sent.clone()).unwrap();
    let second = echo_host_value(first.clone()).unwrap();
    assert_eq!(second, sent);
}

#[test]
fn test_host_values_sent_twice_are_one_value_to_baml() {
    let sent = HostValue::new(3_i32);
    assert!(same_host_value(sent.clone(), sent.clone()).unwrap());
    // Negative control: two host values are two values.
    assert!(!same_host_value(sent, HostValue::new(3_i32)).unwrap());
}

#[test]
fn test_host_value_back_decodes_after_baml_collects_garbage() {
    let sent = HostValue::new(String::from("kept"));
    let back = echo_host_value(sent.clone()).unwrap();
    // Nothing in BAML holds the value any more: the engine gives it back.
    collect_garbage().unwrap();
    assert_eq!(back, sent);
    assert_eq!(
        back.downcast_ref::<String>().map(String::as_str),
        Some("kept")
    );
    // Sent again, it is still the host's one value.
    let again = echo_host_value(back.clone()).unwrap();
    assert_eq!(again, sent);
    assert!(same_host_value(again, back).unwrap());
}
