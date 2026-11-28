// The values both sides of the bench (and the tests) use. The profile is the
// one today's SDK tests round-trip (roundtrip_complex_models.test.ts).
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
} from "./baml_sdk/complex_models/index.js";

export function complexProfile(): ComplexProfile {
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
  return profile;
}
