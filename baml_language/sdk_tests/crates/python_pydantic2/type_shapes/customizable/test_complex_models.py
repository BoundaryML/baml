"""Roundtrip coverage for a complex nested object graph."""

from baml_sdk.complex_models import (
    AccountTier,
    AuditEvent,
    CardPayment,
    ComplexProfile,
    ContactMethod,
    GeoPoint,
    Invoice,
    LineItem,
    PostalAddress,
    ProfileOwner,
    WirePayment,
    round_trip_complex_profile,
)


def test_complex_models_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class():
    home = PostalAddress(
        line1="1 Compiler Way",
        line2=None,
        city="San Francisco",
        region="CA",
        postal_code="94107",
        location=GeoPoint(lat=37.7749, lng=-122.4194),
    )
    office = PostalAddress(
        line1="200 Type Lane",
        line2="Suite 42",
        city="Oakland",
        region="CA",
        postal_code="94612",
        location=None,
    )
    email = ContactMethod(label="email", value="ada@example.com", verified=True)
    phone = ContactMethod(label="phone", value="+1-555-0100", verified=False)
    invoice = Invoice(
        id="inv-001",
        status="sent",
        items=[
            LineItem(
                sku="sdk-pro",
                quantity=2,
                unit_price=19.5,
                tags=["sdk", "typescript"],
                attributes={"language": "ts", "support": "priority"},
            )
        ],
        payment=CardPayment(brand="visa", last4="4242", billing_address=home),
        notes="first invoice",
    )
    wire_invoice = Invoice(
        id="inv-002",
        status="paid",
        items=[
            LineItem(
                sku="sdk-enterprise",
                quantity=1,
                unit_price=250,
                tags=["sdk", "enterprise"],
                attributes={"language": "python", "term": "annual"},
            )
        ],
        payment=WirePayment(
            bank_name="Boundary Bank",
            routing_code="110000000",
            reference=None,
        ),
        notes=None,
    )
    profile = ComplexProfile(
        id="profile-001",
        tier=AccountTier.Enterprise,
        owner=ProfileOwner(
            name="Ada Lovelace",
            primary_contact=email,
            backup_contacts=[phone],
        ),
        addresses=[home, office],
        invoices=[invoice, wire_invoice],
        audit_trail=[
            AuditEvent(
                actor="system",
                action="created",
                context={"source": "fixture"},
            ),
            AuditEvent(
                actor="reviewer",
                action="approved",
                context={"level": "2", "region": "us"},
            ),
        ],
        metadata={"cohort": "beta", "owner_kind": "internal"},
        featured=invoice,
        flags=[7, "manual-review", True],
    )

    assert round_trip_complex_profile(profile) == profile
