// Roundtrip coverage for a complex nested object graph.
// Port of type_shapes/customizable/test_complex_models.py. Deviations:
// - Keyword construction becomes aggregate init in field declaration order
//   (matching the generated header).
// - variant-typed fields (Invoice.payment, ComplexProfile.featured, flags
//   elements) spell a baml::variant alternative explicitly; the spellings
//   below deliberately use the python declaration order, which is legal
//   because baml::variant is order-canonical.
#include <baml_sdk.h>
#include <baml_test.h>

#include <string>

namespace complex_models = baml_sdk::complex_models;
using complex_models::AccountTier;
using complex_models::AuditEvent;
using complex_models::CardPayment;
using complex_models::ComplexProfile;
using complex_models::ContactMethod;
using complex_models::GeoPoint;
using complex_models::Invoice;
using complex_models::LineItem;
using complex_models::PostalAddress;
using complex_models::ProfileOwner;
using complex_models::WirePayment;

using Payment = baml::variant<CardPayment, WirePayment>;
using Featured = baml::variant<Invoice, PostalAddress, std::string>;
using Flag = baml::variant<int64_t, std::string, bool>;

BAML_TEST(
    complex_models_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class) {
  const PostalAddress home{
      "1 Compiler Way", std::nullopt,
      "San Francisco",  "CA",
      "94107",          GeoPoint{37.7749, -122.4194},
  };
  const PostalAddress office{
      "200 Type Lane", "Suite 42", "Oakland", "CA", "94612", std::nullopt,
  };
  const ContactMethod email{"email", "ada@example.com", true};
  const ContactMethod phone{"phone", "+1-555-0100", false};
  const Invoice invoice{
      "inv-001",
      BAML_LIT("sent"){},
      {LineItem{
          "sdk-pro",
          2,
          19.5,
          {"sdk", "typescript"},
          {{"language", "ts"}, {"support", "priority"}},
      }},
      Payment{CardPayment{"visa", "4242", home}},
      "first invoice",
  };
  const Invoice wire_invoice{
      "inv-002",
      BAML_LIT("paid"){},
      {LineItem{
          "sdk-enterprise",
          1,
          250.0,
          {"sdk", "enterprise"},
          {{"language", "python"}, {"term", "annual"}},
      }},
      Payment{WirePayment{"Boundary Bank", "110000000", std::nullopt}},
      std::nullopt,
  };
  const ComplexProfile profile{
      "profile-001",
      AccountTier::Enterprise,
      ProfileOwner{"Ada Lovelace", email, {phone}},
      {home, office},
      {invoice, wire_invoice},
      {
          AuditEvent{"system", BAML_LIT("created"){}, {{"source", "fixture"}}},
          AuditEvent{"reviewer",
                     BAML_LIT("approved"){},
                     {{"level", "2"}, {"region", "us"}}},
      },
      {{"cohort", "beta"}, {"owner_kind", "internal"}},
      Featured{invoice},
      {Flag{int64_t{7}}, Flag{std::string("manual-review")}, Flag{true}},
  };

  BAML_ASSERT(complex_models::round_trip_complex_profile(profile) == profile);
}
