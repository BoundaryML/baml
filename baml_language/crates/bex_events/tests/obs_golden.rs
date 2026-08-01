use bex_events::prof::storage::{BcctHeader, ClockDescriptor};

#[test]
fn observability_fixture_registry_is_versioned_and_append_only() {
    let manifest = include_str!("fixtures/obs/v1/manifest.json");
    let value: serde_json::Value = serde_json::from_str(manifest).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["format_epoch"], "observability-v1");
    assert_eq!(value["status"], "frozen");
    let fixtures = value["fixtures"].as_array().unwrap();
    assert!(fixtures.len() >= 4);
    assert!(fixtures.iter().any(|fixture| {
        fixture["format"] == "canonical-value-node-v1"
            && fixture["identity"]
                .as_str()
                .is_some_and(|path| path.ends_with("canonical_class_person.cid"))
    }));
    assert!(fixtures.iter().any(|fixture| fixture["format"] == "BQF1"));
    assert!(
        fixtures
            .iter()
            .any(|fixture| fixture["format"] == "BCCT1-header")
    );
    assert!(
        fixtures
            .iter()
            .any(|fixture| fixture["format"] == "bamldict-v1")
    );
}

#[test]
fn bcct_v1_header_is_byte_exact_and_frozen() {
    let header = BcctHeader {
        process_euid: [1; 16],
        engine_id: 0x0102_0304_0506_0708,
        session_seg_seq: 7,
        started_epoch_ns: 9,
        clock: ClockDescriptor {
            kind: 1,
            quality: 2,
            tick_ns_numer: 3,
            tick_ns_denom: 4,
        },
        revision_id: [5; 32],
    };
    let expected = decode_hex(include_str!("fixtures/obs/v1/bcct_header.hex"));
    let encoded = header.encode();
    assert_eq!(encoded.as_slice(), expected);
    assert_eq!(BcctHeader::decode(&expected).unwrap(), header);
}

fn decode_hex(input: &str) -> Vec<u8> {
    let input = input.trim();
    assert_eq!(input.len() % 2, 0, "hex fixture must have whole bytes");
    (0..input.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&input[offset..offset + 2], 16).unwrap())
        .collect()
}
