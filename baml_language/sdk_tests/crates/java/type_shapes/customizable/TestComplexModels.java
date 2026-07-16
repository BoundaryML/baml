// Roundtrip coverage for a complex nested object graph.
//
// Port of python_pydantic2/type_shapes/customizable/test_complex_models.py
// — same test names, cases, inputs, assertions.
//
// java-port note: `Invoice.status` (`"draft" | "sent" | "paid"`) and
// `AuditEvent.action` (`"created" | "updated" | "approved"`) are literal
// unions whose arms all share the same underlying primitive (`string`).
// Unlike a heterogeneous union (e.g. `CardPayment | WirePayment`), there's
// no ambiguity to discriminate at runtime — every arm decodes to the exact
// same Java type — so this port assumes codegen collapses a same-base-type
// literal union directly to that base type (`String`) rather than wrapping
// it in a sealed interface, mirroring the doc's "int | int collapses to
// plain int" precedent generalized to literals. This isn't spelled out in
// the conventions doc (literal types aren't in its value table at all) and
// is an invented shape.
//
// `payment` (`CardPayment | WirePayment | null`) and `featured`
// (`Invoice | PostalAddress | string | null`) are genuinely heterogeneous,
// so they use the sealed-interface-per-union shape from TestUnions.java.

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.complex_models.AccountTier;
import baml_sdk.complex_models.AuditEvent;
import baml_sdk.complex_models.CardPayment;
import baml_sdk.complex_models.ComplexProfile;
import baml_sdk.complex_models.ContactMethod;
import baml_sdk.complex_models.Fns;
import baml_sdk.complex_models.GeoPoint;
import baml_sdk.complex_models.Invoice;
import baml_sdk.complex_models.LineItem;
import baml_sdk.complex_models.PostalAddress;
import baml_sdk.complex_models.ProfileOwner;
import baml_sdk.unions$.UnionCardPaymentOrWirePayment;
import baml_sdk.unions$.UnionIntOrStringOrBoolean;
import baml_sdk.unions$.UnionInvoiceOrPostalAddressOrString;
import baml_sdk.complex_models.WirePayment;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class TestComplexModels {

    @Test
    void test_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class() {
        PostalAddress home =
                new PostalAddress(
                        "1 Compiler Way",
                        null,
                        "San Francisco",
                        "CA",
                        "94107",
                        new GeoPoint(37.7749, -122.4194));
        PostalAddress office =
                new PostalAddress(
                        "200 Type Lane", "Suite 42", "Oakland", "CA", "94612", null);
        ContactMethod email = new ContactMethod("email", "ada@example.com", true);
        ContactMethod phone = new ContactMethod("phone", "+1-555-0100", false);
        Invoice invoice =
                new Invoice(
                        "inv-001",
                        "sent",
                        List.of(
                                new LineItem(
                                        "sdk-pro",
                                        2L,
                                        19.5,
                                        List.of("sdk", "typescript"),
                                        Map.of("language", "ts", "support", "priority"))),
                        new UnionCardPaymentOrWirePayment.CardPaymentValue(
                                new CardPayment("visa", "4242", home)),
                        "first invoice");
        Invoice wireInvoice =
                new Invoice(
                        "inv-002",
                        "paid",
                        List.of(
                                new LineItem(
                                        "sdk-enterprise",
                                        1L,
                                        250,
                                        List.of("sdk", "enterprise"),
                                        Map.of("language", "python", "term", "annual"))),
                        new UnionCardPaymentOrWirePayment.WirePaymentValue(
                                new WirePayment("Boundary Bank", "110000000", null)),
                        null);
        ComplexProfile profile =
                new ComplexProfile(
                        "profile-001",
                        AccountTier.Enterprise,
                        new ProfileOwner("Ada Lovelace", email, List.of(phone)),
                        List.of(home, office),
                        List.of(invoice, wireInvoice),
                        List.of(
                                new AuditEvent("system", "created", Map.of("source", "fixture")),
                                new AuditEvent(
                                        "reviewer",
                                        "approved",
                                        Map.of("level", "2", "region", "us"))),
                        Map.of("cohort", "beta", "owner_kind", "internal"),
                        new UnionInvoiceOrPostalAddressOrString.InvoiceValue(invoice),
                        List.of(
                                new UnionIntOrStringOrBoolean.IntValue(7L),
                                new UnionIntOrStringOrBoolean.StringValue("manual-review"),
                                new UnionIntOrStringOrBoolean.BooleanValue(true)));

        assertEquals(profile, Fns.round_trip_complex_profile(profile));
    }
}
