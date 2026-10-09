//! The runtime a BAML program compiled to native Rust links.
//!
//! The Rust backend (`baml_compiler2_rust`) turns a BAML function into an
//! ordinary Rust function over the value types defined here: `int` is
//! [`Int63`], `bigint` is [`BigInt`], `string` is [`Str`], arrays and class
//! instances are [`Shared`]
//! handles with reference semantics, `map<K, V>` is a [`Map`] handle over an
//! insertion-ordered table, `T | null` is `Option<T>`, and every
//! function returns `Result<T, Thrown>` so a `throw` or a panic unwinds as an
//! error value. The language rules both backends must agree on come from
//! [`bex_lang`]; this crate adds only what native code
//! needs and the VM never does: the handle, rendering, JSON through serde,
//! the thrown-value type and the recursion guard. The binary links no VM
//! and no garbage collector.

/// Re-exported so generated code can derive `Serialize`/`Deserialize`
/// (`#[serde(crate = "bex_aot::serde")]`) without its own dependency.
pub use serde;
/// Re-exported for generated code and hosts, see [`json`].
pub use serde_json;

pub mod array;
pub mod bigint;
pub mod depth;
pub mod errors;
pub mod handle;
pub mod json;
pub mod map;
pub mod render;
pub mod string;
mod thrown;

pub use baml_type::Int63;
pub use bex_lang::{Error, Panic, float, int};
pub use bigint::BigInt;
pub use handle::{Shared, ptr_eq, shared};
pub use map::Map;
pub use render::ToBaml;
pub use string::Str;
pub use thrown::{ErrorObject, Thrown, render_object};

/// Narrow a container length to a BAML `int`. Lengths are bounded by
/// `isize::MAX`, which exceeds the i63 range only on a 128-bit target; the
/// saturation is defensive, never reached.
#[inline]
pub(crate) fn int_from_usize(len: usize) -> Int63 {
    i64::try_from(len)
        .ok()
        .and_then(Int63::new)
        .unwrap_or(Int63::MAX)
}
