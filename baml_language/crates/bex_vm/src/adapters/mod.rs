//! Builtin adapters on the shared object format (design section 5).
//!
//! The generated module emits, from each declaration the triage table marks
//! generic or specialized, per-slot-kind functions and a generic adapter that
//! branches on the receiver's descriptor. Both call the one body each builtin
//! has, in `package_baml` (re-exported as `package_baml::bodies`), which the
//! interpreter's glue also runs through `package_baml::value_model`.
//! Compiled code is their consumer; the interpreter calls bodies directly.

#[cfg(test)]
mod tests;

#[allow(
    clippy::too_many_lines,
    reason = "generated: one function per declaration and slot kind"
)]
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/adapters_generated.rs"));
}
