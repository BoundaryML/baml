// The test data of today's Rust SDK test of the complex profile, unchanged
// but for the function around it.

#![allow(
    clippy::too_many_lines,
    clippy::let_and_return,
    unreachable_pub,
    reason = "one value, as the SDK test has it"
)]

use crate::bindings::{
    Map,
    complex_models::{
        AccountTier, AuditEvent, CardPayment, CardPaymentOrWirePayment, ComplexProfile,
        ContactMethod, CreatedOrUpdatedOrApproved, DraftOrSentOrPaid, GeoPoint, IntOrStringOrBool,
        Invoice, InvoiceOrPostalAddressOrString, LineItem, PostalAddress, ProfileOwner,
        WirePayment,
    },
};

/// The profile today's Rust SDK test round trips
/// (`sdk_tests/crates/rust/type_shapes/customizable/test_complex_models.rs`).
pub fn complex_profile() -> ComplexProfile {
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
    profile
}
