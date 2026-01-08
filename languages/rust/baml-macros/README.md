# baml-macros

Derive macros for BAML types.

This crate provides procedural macros (`BamlEncode` and `BamlDecode`) for automatically implementing serialization and deserialization for Rust types used with BAML.

## Usage

This crate provides the `BamlEncode` and `BamlDecode` derive macros. These are typically used via re-exports from the `baml` crate or generated `baml_client` crates:

```rust
use baml::{BamlEncode, BamlDecode};

#[derive(BamlEncode, BamlDecode)]
#[baml(name = "Person")]
struct Person {
    name: String,
    #[baml(name = "years_old")]
    age: i64,
}
```

This crate is a dependency of the `baml` runtime crate. End users typically interact with these macros through re-exports, not directly. For more information, see the [BAML documentation](https://docs.boundaryml.com).

## License

MIT License - see the [LICENSE](../../LICENSE) file for details.

