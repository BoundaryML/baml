// Test stream expansion for enum fields.
// Expected: enum fields get null | EnumName (NOT stream_EnumName).

enum Status {
  Active
  Inactive
}

class WithEnum {
  status Status
}
