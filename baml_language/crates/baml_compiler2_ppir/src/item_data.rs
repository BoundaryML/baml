//! Firewall queries — the public way to read one item.
//!
//! `file_semantic_index` is `no_eq`: it re-runs on every edit to the file and
//! always reports "changed". Anything that reads the `ItemTree` through it
//! therefore re-runs on every keystroke, however unrelated the edit. These
//! queries are the firewall: they re-run cheaply, but their *results* are
//! compared with `PartialEq`, so a downstream query only re-runs when the item
//! it actually depends on changed.
//!
//! That only works if the result is span-free. Salsa keeps the old memoized
//! value whenever the new one compares equal (`salsa::update` — "this may cause
//! us not to update even if the value has changed"), so a result carrying spans
//! would hand out *stale* spans forever after a whitespace-only edit. Hence
//! every item is split in two:
//!
//! - `*_data` — semantic, span-free, uses [`TypeRef`](baml_compiler2_hir::type_ref::TypeRef).
//!   This is what type checking reads.
//! - `*_source_map` — spans only. This is what diagnostics and the IDE read.
//!
//! The `ItemTree` itself may keep its spans: it lives behind the `no_eq` index,
//! which is overwritten wholesale on every revision, so its spans are always
//! fresh. It is memoized *results* that must be span-free.
//!
//! Each query takes a `*Loc` and nothing else — the file is inside the `Loc`, so
//! callers never thread a `SourceFile` through to reach an item.
//!
//! One submodule per item kind, mirroring `item_tree`.

mod classes;
mod clients;
mod common;
mod enumeration;
mod enums;
mod functions;
mod impls;
mod interfaces;
mod lets;
mod retry_policies;
mod scopes;
mod template_strings;
mod test_items;
mod type_aliases;

pub use classes::*;
pub use clients::*;
pub use common::*;
pub use enumeration::*;
pub use enums::*;
pub use functions::*;
pub use impls::*;
pub use interfaces::*;
pub use lets::*;
pub use retry_policies::*;
pub use scopes::*;
pub use template_strings::*;
pub use test_items::*;
pub use type_aliases::*;
