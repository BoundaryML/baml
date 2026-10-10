Question
  What does a useful data-transformation job cost?

Work
  Parse records, validate ASCII account names, integer-valued cents in
  [0,1000000] and boolean active flags; group active amounts; sort and encode.

BAML path
  Native JSON parsing, typed heap values, maps, iteration, strings and encoding.

Boundary
  Input text is prepared outside execution; parse/validate/group/sort/encode
  are inside every job. Invalid input returns {"error":"invalid"}.

Evidence
  Full result matches the independent oracle. Invalid fields and syntax
  are checked. No duplicate-key or invalid-Unicode equivalence is claimed.

Claim
  Application CPU and wall time per correct serialized job. Native-library
  work remains in the result; faster JSON does not alone prove better MIR code.

References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_vm/src/package_baml/json.rs
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/bex_heap/src/tlab.rs
