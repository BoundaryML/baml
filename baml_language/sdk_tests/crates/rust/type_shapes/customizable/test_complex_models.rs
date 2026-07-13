//! Roundtrip coverage for a complex nested object graph.

// PROVISIONAL(rust-codegen): BAML's anonymous unions have no final Rust
// naming yet (python renders them structurally as `Union[...]`/`Literal[...]`,
// which Rust cannot). This port assumes a synthesized enum per union site,
// named `<Class><FieldCamelCase>` (`<Class><FieldCamelCase>Item` for a
// list-element union), with one variant per arm — literal-string arms as
// CamelCase unit variants, class/primitive arms wrapping their payload — and
// a trailing `| null` arm lowering to `Option<...>` around the enum. Expect
// fixups at flip time.
use baml_rs::Map;
use baml_sdk::complex_models::{
    AccountTier, AuditEvent, AuditEventAction, CardPayment, ComplexProfile, ComplexProfileFeatured,
    ComplexProfileFlagsItem, ContactMethod, GeoPoint, Invoice, InvoicePayment, InvoiceStatus,
    LineItem, PostalAddress, ProfileOwner, WirePayment, round_trip_complex_profile,
};

#[test]
fn test_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class() {
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
        status: InvoiceStatus::Sent,
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
        payment: Some(InvoicePayment::CardPayment(CardPayment {
            brand: "visa".to_string(),
            last4: "4242".to_string(),
            billing_address: home.clone(),
        })),
        notes: Some("first invoice".to_string()),
    };
    let wire_invoice = Invoice {
        id: "inv-002".to_string(),
        status: InvoiceStatus::Paid,
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
        payment: Some(InvoicePayment::WirePayment(WirePayment {
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
                action: AuditEventAction::Created,
                context: Map::from([("source".to_string(), "fixture".to_string())]),
            },
            AuditEvent {
                actor: "reviewer".to_string(),
                action: AuditEventAction::Approved,
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
        featured: Some(ComplexProfileFeatured::Invoice(invoice)),
        flags: vec![
            ComplexProfileFlagsItem::Int(7),
            ComplexProfileFlagsItem::String("manual-review".to_string()),
            ComplexProfileFlagsItem::Bool(true),
        ],
    };

    assert_eq!(
        round_trip_complex_profile(profile.clone()).unwrap(),
        profile
    );
}
