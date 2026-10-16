JSON hello world

Question
  What does returning a tiny JSON object cost through each runtime adapter?
Work
  Encode {"message":"Hello, world!"} on every invocation. Correctness fixtures
  also contain empty strings, quotes, a newline and non-ASCII text.
Boundary
  Input decoding occurs in prepare. Object serialization, adapter entry and
  returning the string occur inside execution. No terminal or network I/O is timed.
BAML path
  Typed JSON decoding during setup, a VM-to-native call and class serialization.
Evidence
  Every process's last returned document is structurally checked. Escaping syntax
  may differ while representing the same string. No GC or steady-state claim.
Claim
  This complements integer startup; it is not an HTTP request or JSON parser test.
References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/baml_builtins2/baml_std/baml/ns_json/json.baml
