// Roundtrip coverage for a complex nested object graph.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
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
} from "./baml_sdk/complex_models/index.js";

describe("roundtrip complex_models", () => {
  it("complex_models_round_trip_complex_profile_preserves_deeply_nested_mixed_shape_class", () => {
    const home = new PostalAddress({
      line1: "1 Compiler Way",
      line2: null,
      city: "San Francisco",
      region: "CA",
      postal_code: "94107",
      location: new GeoPoint({ lat: 37.7749, lng: -122.4194 }),
    });
    const office = new PostalAddress({
      line1: "200 Type Lane",
      line2: "Suite 42",
      city: "Oakland",
      region: "CA",
      postal_code: "94612",
      location: null,
    });
    const email = new ContactMethod({
      label: "email",
      value: "ada@example.com",
      verified: true,
    });
    const phone = new ContactMethod({
      label: "phone",
      value: "+1-555-0100",
      verified: false,
    });
    const invoice = new Invoice({
      id: "inv-001",
      status: "sent",
      items: [
        new LineItem({
          sku: "sdk-pro",
          quantity: 2,
          unit_price: 19.5,
          tags: ["sdk", "typescript"],
          attributes: { language: "ts", support: "priority" },
        }),
      ],
      payment: new CardPayment({
        brand: "visa",
        last4: "4242",
        billing_address: home,
      }),
      notes: "first invoice",
    });
    const wireInvoice = new Invoice({
      id: "inv-002",
      status: "paid",
      items: [
        new LineItem({
          sku: "sdk-enterprise",
          quantity: 1,
          unit_price: 250,
          tags: ["sdk", "enterprise"],
          attributes: { language: "python", term: "annual" },
        }),
      ],
      payment: new WirePayment({
        bank_name: "Boundary Bank",
        routing_code: "110000000",
        reference: null,
      }),
      notes: null,
    });
    const profile = new ComplexProfile({
      id: "profile-001",
      tier: AccountTier.Enterprise,
      owner: new ProfileOwner({
        name: "Ada Lovelace",
        primary_contact: email,
        backup_contacts: [phone],
      }),
      addresses: [home, office],
      invoices: [invoice, wireInvoice],
      audit_trail: [
        new AuditEvent({
          actor: "system",
          action: "created",
          context: { source: "fixture" },
        }),
        new AuditEvent({
          actor: "reviewer",
          action: "approved",
          context: { level: "2", region: "us" },
        }),
      ],
      metadata: { cohort: "beta", owner_kind: "internal" },
      featured: invoice,
      flags: [7, "manual-review", true],
    });

    expect(round_trip_complex_profile(profile)).toEqual(profile);
  });

  it("complex_models_round_trip_complex_profile_accepts_plain_object_literals_no_class_constructors", () => {
    // The encoder treats a plain object and a `new X({...})` instance
    // identically — both become a tagless `map_value` that the engine coerces
    // to the declared parameter type. So the whole graph can be one nested
    // object literal; the single `: ComplexProfile` annotation gives every
    // nested literal its contextual type, so tsc verifies the shapes. Enum
    // members (`AccountTier.Enterprise`) are values, not constructors.
    const profile: ComplexProfile = {
      id: "profile-001",
      tier: AccountTier.Enterprise,
      owner: {
        name: "Ada Lovelace",
        primary_contact: { label: "email", value: "ada@example.com", verified: true },
        backup_contacts: [{ label: "phone", value: "+1-555-0100", verified: false }],
      },
      addresses: [
        {
          line1: "1 Compiler Way",
          line2: null,
          city: "San Francisco",
          region: "CA",
          postal_code: "94107",
          location: { lat: 37.7749, lng: -122.4194 },
        },
        {
          line1: "200 Type Lane",
          line2: "Suite 42",
          city: "Oakland",
          region: "CA",
          postal_code: "94612",
          location: null,
        },
      ],
      invoices: [
        {
          id: "inv-001",
          status: "sent",
          items: [
            {
              sku: "sdk-pro",
              quantity: 2,
              unit_price: 19.5,
              tags: ["sdk", "typescript"],
              attributes: { language: "ts", support: "priority" },
            },
          ],
          payment: {
            brand: "visa",
            last4: "4242",
            billing_address: {
              line1: "1 Compiler Way",
              line2: null,
              city: "San Francisco",
              region: "CA",
              postal_code: "94107",
              location: { lat: 37.7749, lng: -122.4194 },
            },
          },
          notes: "first invoice",
        },
        {
          id: "inv-002",
          status: "paid",
          items: [
            {
              sku: "sdk-enterprise",
              quantity: 1,
              unit_price: 250,
              tags: ["sdk", "enterprise"],
              attributes: { language: "python", term: "annual" },
            },
          ],
          payment: {
            bank_name: "Boundary Bank",
            routing_code: "110000000",
            reference: null,
          },
          notes: null,
        },
      ],
      audit_trail: [
        { actor: "system", action: "created", context: { source: "fixture" } },
        { actor: "reviewer", action: "approved", context: { level: "2", region: "us" } },
      ],
      metadata: { cohort: "beta", owner_kind: "internal" },
      featured: {
        id: "inv-001",
        status: "sent",
        items: [
          {
            sku: "sdk-pro",
            quantity: 2,
            unit_price: 19.5,
            tags: ["sdk", "typescript"],
            attributes: { language: "ts", support: "priority" },
          },
        ],
        payment: {
          brand: "visa",
          last4: "4242",
          billing_address: {
            line1: "1 Compiler Way",
            line2: null,
            city: "San Francisco",
            region: "CA",
            postal_code: "94107",
            location: { lat: 37.7749, lng: -122.4194 },
          },
        },
        notes: "first invoice",
      },
      flags: [7, "manual-review", true],
    };

    const result = round_trip_complex_profile(profile);

    // `toEqual` passes because it checks *structural* equality: it recurses
    // through field values and ignores both object identity and the
    // prototype/class. `result` is a freshly decoded `ComplexProfile`
    // *instance*; `profile` is a plain object literal — equal in value, but a
    // different object of a different type.
    expect(result).toEqual(profile);

    // The distinction `toEqual` ignores: the decoded value carries the
    // generated class prototype, while the literal carries `Object.prototype`.
    expect(result).toBeInstanceOf(ComplexProfile);
    expect(Object.getPrototypeOf(result)).not.toBe(
      Object.getPrototypeOf(profile),
    );

    // `toStrictEqual` additionally checks the type (constructor), so it fails
    // on that prototype mismatch even though every field value matches.
    expect(result).not.toStrictEqual(profile);
  });
});
