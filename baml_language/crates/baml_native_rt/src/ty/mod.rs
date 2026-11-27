//! Types and interface rules, as the runtime holds them: its own copies of
//! what it needs of `baml_type` and `bex_vm_types`, with layouts the
//! compiler can write into the executable and children that are static or
//! owned (`design/memory.md`, "Types and rules are copies").
//!
//! Every file here but this one and `data.rs` is a copy, and says of what.
//! A change to a copy or to its original must be made to both.

#![allow(
    dead_code,
    unreachable_pub,
    reason = "nothing calls in yet; the compiler includes the type files by path and reads them as its own"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::trivially_copy_pass_by_ref,
    clippy::unnecessary_wraps,
    reason = "the copies keep the original's code and comments as they are, where a head was a name passed by reference"
)]

mod core_types;
mod data;
mod family;
mod freshness;
mod interface;
mod normalize;
mod template;

pub use core_types::{Literal, MediaKind};
pub use data::{
    AssocBinding, AssocIndex, ClassObject, CowBox, CowSlice, EnumVariantObject, InterfaceObject,
    PackageObject, RawFunctionObject, SlotIndex, Text, TypeObject,
};
pub use family::{FunctionParamTy, RealizedTy, TyTemplate};
pub use freshness::{Freshness, FunctionParamMode};
pub use interface::InterfaceBound;
