//! Roundtrip coverage for a complex nested object graph.

// Anonymous unions surface as one synthesized enum per (null-stripped)
// union shape, named by joining the arm names with `Or` — literal-string
// arms as unit variants from their value, class/primitive arms wrapping
// their payload — and a trailing `| null` lowering to `Option<...>`
// around the enum.
use baml_bridge::{
    Map, baml_value::internal::__BamlValuePrivate, wire::inbound_value::Value as WireValue,
};
use baml_sdk::complex_models::{
    AccountTier, AuditEvent, CardPayment, CardPaymentOrWirePayment, ComplexProfile, ContactMethod,
    CreatedOrUpdatedOrApproved, DraftOrSentOrPaid, GeoPoint, GraphQueryOrGraphDiff,
    IntOrStringOrBool, Invoice, InvoiceOrPostalAddressOrString, LineItem,
    LiteralUnionIdentifierEdges, PostalAddress, ProfileOwner, Utf8LossyOrUtf8Strict, WirePayment,
    round_trip_complex_profile, round_trip_literal_union_identifier_edges,
};

// SDK_PARITY_LINT(skip): exercises Rust identifier synthesis for string-literal union arms
#[test]
fn test_complex_models_round_trip_non_identifier_string_literal_union_arms() {
    for (command, command_wire_value) in [
        (GraphQueryOrGraphDiff::GraphQuery, "graph.query"),
        (GraphQueryOrGraphDiff::GraphDiff, "graph.diff"),
    ] {
        assert_string_wire_value(&command, command_wire_value);
        for (encoding, encoding_wire_value) in [
            (Utf8LossyOrUtf8Strict::Utf8Lossy, "utf8-lossy"),
            (Utf8LossyOrUtf8Strict::Utf8Strict, "utf8-strict"),
        ] {
            assert_string_wire_value(&encoding, encoding_wire_value);
            let value = LiteralUnionIdentifierEdges {
                command: command.clone(),
                encoding,
            };
            assert_eq!(
                round_trip_literal_union_identifier_edges(value.clone()).unwrap(),
                value
            );
        }
    }
}

fn assert_string_wire_value<T: __BamlValuePrivate>(value: &T, expected: &str) {
    match value.to_baml().value {
        Some(WireValue::StringValue(actual)) => assert_eq!(actual, expected),
        other => panic!("expected string wire value {expected:?}, got {other:?}"),
    }
}

#[test]
fn test_complex_models_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class() {
    let home = PostalAddress {
        line1: "1 Compiler Way".to_string(),
        line2: None,
        city: "San Francisco".to_string(),
        region: "CA".to_string(),
        postal_code: "94107".to_string(),
        location: Some(GeoPoint {
            lat: 37.7749,
            lng: -122.4194,
        }),
    };
    let office = PostalAddress {
        line1: "200 Type Lane".to_string(),
        line2: Some("Suite 42".to_string()),
        city: "Oakland".to_string(),
        region: "CA".to_string(),
        postal_code: "94612".to_string(),
        location: None,
    };
    let email = ContactMethod {
        label: "email".to_string(),
        value: "ada@example.com".to_string(),
        verified: true,
    };
    let phone = ContactMethod {
        label: "phone".to_string(),
        value: "+1-555-0100".to_string(),
        verified: false,
    };
    let invoice = Invoice {
        id: "inv-001".to_string(),
        status: DraftOrSentOrPaid::Sent,
        items: vec![LineItem {
            sku: "sdk-pro".to_string(),
            quantity: 2,
            unit_price: 19.5,
            tags: vec!["sdk".to_string(), "typescript".to_string()],
            attributes: Map::from([
                ("language".to_string(), "ts".to_string()),
                ("support".to_string(), "priority".to_string()),
            ]),
        }],
        payment: Some(CardPaymentOrWirePayment::CardPayment(CardPayment {
            brand: "visa".to_string(),
            last4: "4242".to_string(),
            billing_address: home.clone(),
        })),
        notes: Some("first invoice".to_string()),
    };
    let wire_invoice = Invoice {
        id: "inv-002".to_string(),
        status: DraftOrSentOrPaid::Paid,
        items: vec![LineItem {
            sku: "sdk-enterprise".to_string(),
            quantity: 1,
            // DIVERGENCE(rust): python passes the int literal 250 and relies
            // on pydantic's construction-time int→float coercion; Rust writes
            // the float value directly.
            unit_price: 250.0,
            tags: vec!["sdk".to_string(), "enterprise".to_string()],
            attributes: Map::from([
                ("language".to_string(), "python".to_string()),
                ("term".to_string(), "annual".to_string()),
            ]),
        }],
        payment: Some(CardPaymentOrWirePayment::WirePayment(WirePayment {
            bank_name: "Boundary Bank".to_string(),
            routing_code: "110000000".to_string(),
            reference: None,
        })),
        notes: None,
    };
    let profile = ComplexProfile {
        id: "profile-001".to_string(),
        tier: AccountTier::Enterprise,
        owner: ProfileOwner {
            name: "Ada Lovelace".to_string(),
            primary_contact: email,
            backup_contacts: vec![phone],
        },
        addresses: vec![home, office],
        invoices: vec![invoice.clone(), wire_invoice],
        audit_trail: vec![
            AuditEvent {
                actor: "system".to_string(),
                action: CreatedOrUpdatedOrApproved::Created,
                context: Map::from([("source".to_string(), "fixture".to_string())]),
            },
            AuditEvent {
                actor: "reviewer".to_string(),
                action: CreatedOrUpdatedOrApproved::Approved,
                context: Map::from([
                    ("level".to_string(), "2".to_string()),
                    ("region".to_string(), "us".to_string()),
                ]),
            },
        ],
        metadata: Map::from([
            ("cohort".to_string(), "beta".to_string()),
            ("owner_kind".to_string(), "internal".to_string()),
        ]),
        featured: Some(InvoiceOrPostalAddressOrString::Invoice(invoice)),
        flags: vec![
            IntOrStringOrBool::Int(7),
            IntOrStringOrBool::String("manual-review".to_string()),
            IntOrStringOrBool::Bool(true),
        ],
    };

    assert_eq!(
        round_trip_complex_profile(profile.clone()).unwrap(),
        profile
    );
}
