# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class ComplexModelsTest < Minitest::Test
  Models = BamlSdk::ComplexModels

  def test_complex_models_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class
    home = Models::PostalAddress.new(
      line1: "1 Compiler Way", line2: nil, city: "San Francisco", region: "CA", postal_code: "94107",
      location: Models::GeoPoint.new(lat: 37.7749, lng: -122.4194)
    )
    office = Models::PostalAddress.new(
      line1: "200 Type Lane", line2: "Suite 42", city: "Oakland", region: "CA", postal_code: "94612", location: nil
    )
    email = Models::ContactMethod.new(label: "email", value: "ada@example.com", verified: true)
    phone = Models::ContactMethod.new(label: "phone", value: "+1-555-0100", verified: false)
    invoice = Models::Invoice.new(
      id: "inv-001", status: "sent",
      items: [Models::LineItem.new(
        sku: "sdk-pro", quantity: 2, unit_price: 19.5, tags: ["sdk", "typescript"],
        attributes: { "language" => "ts", "support" => "priority" }
      )],
      payment: Models::CardPayment.new(brand: "visa", last4: "4242", billing_address: home),
      notes: "first invoice"
    )
    wire_invoice = Models::Invoice.new(
      id: "inv-002", status: "paid",
      items: [Models::LineItem.new(
        sku: "sdk-enterprise", quantity: 1, unit_price: 250, tags: ["sdk", "enterprise"],
        attributes: { "language" => "python", "term" => "annual" }
      )],
      payment: Models::WirePayment.new(bank_name: "Boundary Bank", routing_code: "110000000", reference: nil),
      notes: nil
    )
    profile = Models::ComplexProfile.new(
      id: "profile-001", tier: Models::AccountTier::Enterprise,
      owner: Models::ProfileOwner.new(name: "Ada Lovelace", primary_contact: email, backup_contacts: [phone]),
      addresses: [home, office], invoices: [invoice, wire_invoice],
      audit_trail: [
        Models::AuditEvent.new(actor: "system", action: "created", context: { "source" => "fixture" }),
        Models::AuditEvent.new(actor: "reviewer", action: "approved", context: { "level" => "2", "region" => "us" })
      ],
      metadata: { "cohort" => "beta", "owner_kind" => "internal" }, featured: invoice,
      flags: [7, "manual-review", true]
    )
    assert_equal profile, Models.round_trip_complex_profile(profile)
  end
end
