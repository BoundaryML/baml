# baml_num

`Int63`, the 63-bit `int` of the BAML language, with its checked arithmetic,
shifts and bit operations, and the `bigint` size caps. No dependencies.

`baml_type` re-exports everything here, so nothing in the compiler or the VM
changes. The crate exists so that code which needs only the integer, such as
a BAML program compiled to native Rust, can link it without `baml_type` and
the compiler infrastructure underneath it.

The optional `serde` feature gives `Int63` a plain integer encoding with the
i63 range check.
