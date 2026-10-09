//! Language semantics shared by the BAML VM and generated native code.
//!
//! Both backends must agree on what an `int` overflow is and says, how a
//! negative index counts from the end, how a `float` becomes an `int`, which
//! `baml.panics.*` class a failure materializes as and how an exit code is
//! narrowed. Each of those is written once here, over `baml_type`'s
//! [`Int63`], for both backends to call; generated native code already does,
//! and the VM's builtins switch to it in a separate change. Nothing here knows
//! about a heap, a `Value` or a Rust handle: native-only representations live
//! in `bex_aot`.

pub use baml_type::{Int63, IntShiftError};

mod error;
mod exit;
pub mod float;
pub mod index;
pub mod int;
mod panic;

pub use error::Error;
pub use exit::clamp_exit_code;
pub use panic::{Panic, render_object};
