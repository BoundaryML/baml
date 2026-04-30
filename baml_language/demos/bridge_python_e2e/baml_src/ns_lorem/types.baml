class Address {
  street string
  city string
  zip string?
}

enum Sentiment {
  POSITIVE
  NEGATIVE
  NEUTRAL
}

class PhoneNumber {
  country_code int
  digits string
}

class Resume {
  name string
  email string?
  addresses Address[]
  scores map<string, int>
  sentiment Sentiment
  contact PhoneNumber

  // Instance method — class methods are emitted unconditionally
  // (no LLM gating in `build_symbol_pool`), so this is the cleanest
  // way to expose a pure expression function from generated Python
  // today.
  function transform(self) -> Resume {
    Resume {
      name: self.name,
      email: self.email,
      addresses: self.addresses,
      scores: self.scores,
      // Hardcoded flip rather than a conditional on `self.sentiment`,
      // because BAML enum-variant `==` doesn't currently report equality
      // for like variants in the bex_engine path. Filed as follow-up.
      sentiment: Sentiment.NEGATIVE,
      contact: self.contact,
    }
  }
}

// Generic container — exercises 13a's codegen for `Generic[T]` and 13b's
// outbound parameterization. `repackage` returns the same shape so the
// engine round-trips an instance whose declared type is generic.
class Box<T> {
  item T

  function repackage(self) -> Box<T> {
    Box<T> { item: self.item }
  }
}
