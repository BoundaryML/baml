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
// same Java type — so codegen erases a same-base-type literal union directly
// to that base type (`String`) rather than wrapping it, per the conventions
// doc's "Same-base literal unions still erase to the base type" rule.
//
// `payment` (`CardPayment | WirePayment | null`) -> `Union2<CardPayment,
// WirePayment>` and `featured` (`Invoice | PostalAddress | string | null`)
// -> `Union3<Invoice, PostalAddress, String>` are genuinely heterogeneous,
// so they use the generic-family shape from TestUnions.java (null arm
// stripped, arms in BAML declaration order). `flags` (`(int | string |
// bool)[]`) -> `List<Union3<Long, String, Boolean>>`.

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_bridge.Union2;
import baml_bridge.Union3;
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
import baml_sdk.complex_models.WirePayment;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class TestComplexModels {

    @Test
    void test_complex_models_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class() {
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
                        new Union2.Arm0<CardPayment, WirePayment>(
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
                        new Union2.Arm1<CardPayment, WirePayment>(
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
                        new Union3.Arm0<Invoice, PostalAddress, String>(invoice),
                        List.of(
                                new Union3.Arm0<Long, String, Boolean>(7L),
                                new Union3.Arm1<Long, String, Boolean>("manual-review"),
                                new Union3.Arm2<Long, String, Boolean>(true)));

        assertEquals(profile, Fns.round_trip_complex_profile(profile));
    }
}
