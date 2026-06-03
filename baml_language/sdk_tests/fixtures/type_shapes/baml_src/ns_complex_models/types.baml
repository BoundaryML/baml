// Complex nested model coverage for host SDK round trips.
//
// This namespace intentionally combines several type-shape features in one
// object graph: multiple scalar fields, nested classes, nested lists/maps,
// optional fields, enum fields, literal unions, class unions, and mixed
// primitive/class unions.

enum AccountTier {
  Free,
  Pro,
  Enterprise,
}

class GeoPoint {
  lat float
  lng float
}

class PostalAddress {
  line1 string
  line2 string?
  city string
  region string
  postal_code string
  location GeoPoint?
}

class ContactMethod {
  label string
  value string
  verified bool
}

class ProfileOwner {
  name string
  primary_contact ContactMethod
  backup_contacts ContactMethod[]
}

class CardPayment {
  brand string
  last4 string
  billing_address PostalAddress
}

class WirePayment {
  bank_name string
  routing_code string
  reference string?
}

class LineItem {
  sku string
  quantity int
  unit_price float
  tags string[]
  attributes map<string, string>
}

class Invoice {
  id string
  status "draft" | "sent" | "paid"
  items LineItem[]
  payment CardPayment | WirePayment | null
  notes string?
}

class AuditEvent {
  actor string
  action "created" | "updated" | "approved"
  context map<string, string>
}

class ComplexProfile {
  id string
  tier AccountTier
  owner ProfileOwner
  addresses PostalAddress[]
  invoices Invoice[]
  audit_trail AuditEvent[]
  metadata map<string, string>
  featured Invoice | PostalAddress | string | null
  flags (int | string | bool)[]
}

function round_trip_complex_profile(profile: ComplexProfile) -> ComplexProfile {
  profile
}
