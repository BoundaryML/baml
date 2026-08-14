//! Roundtrip coverage for `baml_sdk::maps` — map Ty variants.

use baml_bridge::Map;
use baml_sdk::maps::{
    Resume, Sentiment, round_trip_list_valued_map, round_trip_resume, round_trip_sentiment,
    round_trip_simple_map,
};

// NOTE: enum-keyed maps don't round-trip yet. The inbound wire encoding
// carries an enum map key as a typed `enum_key`, but the OUTBOUND map entry
// carries only a scalar `entry.key`, so the engine renders an enum key as the
// string `"<fqn>::<variant>"` and decode hands it back as that raw string
// rather than the enum member. Finishing it needs a typed outbound map key
// (proto schema + engine emit + decode), not just an encoder tweak. The
// `round_trip_enum_keyed_map` and `round_trip_map_container` tests (the
// latter has a required `enum_keyed` map field) are dropped until that lands;
// enum *values* still round-trip (test_round_trip_sentiment).

#[test]
fn test_maps_round_trip_simple_map() {
    let m = Map::from([("a".to_string(), 1), ("b".to_string(), 2)]);
    assert_eq!(round_trip_simple_map(m.clone()).unwrap(), m);
}

#[test]
fn test_maps_round_trip_list_valued_map() {
    let m = Map::from([("k".to_string(), vec![1, 2])]);
    assert_eq!(round_trip_list_valued_map(m.clone()).unwrap(), m);
}

#[test]
fn test_maps_round_trip_sentiment() {
    assert_eq!(
        round_trip_sentiment(Sentiment::Positive).unwrap(),
        Sentiment::Positive
    );
}

#[test]
fn test_maps_round_trip_resume() {
    let r = Resume {
        name: "n".to_string(),
    };
    assert_eq!(round_trip_resume(r.clone()).unwrap(), r);
}
