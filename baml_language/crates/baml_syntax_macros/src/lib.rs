//! Procedural macros backing `baml_syntax`'s typed tree.
//!
//! The tree is a set of plain node structs stored in per-type arenas. These
//! macros generate the mechanical parts so that the structs themselves are
//! the single source of truth for the grammar's shape:
//!
//! - [`define_tree!`](define_tree) — given the list of node types and list
//!   pools, generates `NodeKind`, the `Arenas` struct with one arena per node
//!   type (the builder), `AnyNodeRef`, the read-only `FileTree`, the per-type
//!   `NodeKindOf` impls that give every node its kind, and
//!   `for_each_node_type!` (which needs `nodes::*` and `tree::Bogus` in
//!   scope).
//! - [`Node`](derive@Node) — on an arena node struct: walks its fields in
//!   declaration order (the grammar's source order) and marks the type as a
//!   node. A node not listed in `define_tree!` has no kind and cannot be
//!   allocated; a listed type without this derive has no arena. Both are
//!   compile errors, so the list and the structs cannot drift apart.
//! - [`Walk`](derive@Walk) — on an inline struct held by value inside a node
//!   (a label, a clause); walked transparently as part of its parent.
//! - [`AnyNode`](derive@AnyNode) — on a union enum of alternatives; walks the
//!   active variant and must carry a `Bogus` variant, so every union can hold
//!   recovered garbage without loss.
//!
//! - [`TokenTable`](derive@TokenTable) — on the token kind enum, which also
//!   derives `Logos`: reads the same `#[token]`/`#[regex]` attributes plus
//!   `#[group(…)]`/`#[describe]`/`#[spelling]` and emits `ALL`, the group predicates,
//!   `describe()`, the `Token<K>` marker types and the `Token!`/`TokenKind!`
//!   macros — so the token vocabulary is written exactly once.
//!
//! Node fields may not allocate (`Vec`, `Box`, `String`, `Rc`, `Arc` are
//! rejected anywhere in a field's type, by name): children are `Id<T>`,
//! tokens are `Token<K>`, sequences are `List<T>` into a shared pool. That
//! keeps a node a few words wide and the whole tree three flat allocations.
//! Every node type derives `Debug`, `Clone`, `PartialEq`, `Eq` and `Hash`,
//! which `Arenas` derives in turn.
//!
//! The node traits are sealed, so these macros apply inside `baml_syntax`
//! only; it does not re-export them.

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

mod derive;
mod token_table;
mod tree;

/// Declare the tree: `define_tree! { root: SourceFile, nodes { … }, pools { … } }`.
#[proc_macro]
pub fn define_tree(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as tree::TreeInput);
    tree::emit(&input).into()
}

/// Mark an arena node and walk its fields in declaration order.
#[proc_macro_derive(Node)]
pub fn derive_node(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive::node(&input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Walk an inline struct's fields in declaration order.
#[proc_macro_derive(Walk)]
pub fn derive_walk(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive::walk(&input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Walk the active variant of a union enum; requires a `Bogus` variant.
#[proc_macro_derive(AnyNode)]
pub fn derive_any_node(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive::any_node(&input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Derive the token table from the token kind enum.
#[proc_macro_derive(TokenTable, attributes(group, describe, spelling))]
pub fn derive_token_table(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    token_table::token_table(&input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
