//! Builtin adapters on the shared object format (design section 5).
//!
//! `bodies` holds one Rust body per builtin, generic over the slot
//! representation. The generated module emits, from each declaration the
//! triage table marks generic or specialized, per-slot-kind functions and a
//! generic adapter that branches on the receiver's descriptor. `vm_model`
//! implements the layout ABI over today's heap so the adapters run against
//! the current VM; nothing in the interpreter dispatches to them yet.

mod bodies;
mod support;
pub mod vm_model;

#[cfg(test)]
mod tests;

#[allow(
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    dead_code,
    reason = "generated: one function per declaration and slot kind"
)]
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/adapters_generated.rs"));
}
