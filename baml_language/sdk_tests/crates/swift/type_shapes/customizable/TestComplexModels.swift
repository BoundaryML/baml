// Roundtrip coverage for a complex nested object graph — port of
// python_pydantic2 `test_complex_models.py`, on the BamlUnionN family.
//
// Union-typed slots take positional cases (`payment: .t0(card)`);
// literal-union fields (`status`, `action`) collapse to String (no
// generated literal enums — the engine validates values).
import XCTest
import Baml
import BamlBridge

final class TestComplexModels: XCTestCase {
    func test_complex_models_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class() throws {
        typealias M = Baml.complex_models

        let home = M.PostalAddress(
            line1: "1 Compiler Way",
            line2: nil,
            city: "San Francisco",
            region: "CA",
            postal_code: "94107",
            location: M.GeoPoint(lat: 37.7749, lng: -122.4194)
        )
        let office = M.PostalAddress(
            line1: "200 Type Lane",
            line2: "Suite 42",
            city: "Oakland",
            region: "CA",
            postal_code: "94612",
            location: nil
        )
        let email = M.ContactMethod(label: "email", value: "ada@example.com", verified: true)
        let phone = M.ContactMethod(label: "phone", value: "+1-555-0100", verified: false)
        let invoice = M.Invoice(
            id: "inv-001",
            status: "sent",
            items: [
                M.LineItem(
                    sku: "sdk-pro",
                    quantity: 2,
                    unit_price: 19.5,
                    tags: ["sdk", "typescript"],
                    attributes: ["language": "ts", "support": "priority"]
                )
            ],
            payment: .t0(M.CardPayment(brand: "visa", last4: "4242", billing_address: home)),
            notes: "first invoice"
        )
        let wireInvoice = M.Invoice(
            id: "inv-002",
            status: "paid",
            items: [
                M.LineItem(
                    sku: "sdk-enterprise",
                    quantity: 1,
                    unit_price: 250,
                    tags: ["sdk", "enterprise"],
                    attributes: ["language": "python", "term": "annual"]
                )
            ],
            payment: .t1(
                M.WirePayment(
                    bank_name: "Boundary Bank",
                    routing_code: "110000000",
                    reference: nil
                )
            ),
            notes: nil
        )
        let profile = M.ComplexProfile(
            id: "profile-001",
            tier: .Enterprise,
            owner: M.ProfileOwner(
                name: "Ada Lovelace",
                primary_contact: email,
                backup_contacts: [phone]
            ),
            addresses: [home, office],
            invoices: [invoice, wireInvoice],
            audit_trail: [
                M.AuditEvent(actor: "system", action: "created", context: ["source": "fixture"]),
                M.AuditEvent(
                    actor: "reviewer",
                    action: "approved",
                    context: ["level": "2", "region": "us"]
                ),
            ],
            metadata: ["cohort": "beta", "owner_kind": "internal"],
            featured: .t0(invoice),
            flags: [.t0(7), .t1("manual-review"), .t2(true)]
        )

        let result = try M.round_trip_complex_profile(profile: profile)
        XCTAssertEqual(result, profile)

        // Class-arm identity survives the round trip: the wire says
        // WHICH payment class was selected, decode doesn't guess.
        XCTAssertNotNil(result.invoices[0].payment?.t0)
        XCTAssertNotNil(result.invoices[1].payment?.t1)
    }
}
