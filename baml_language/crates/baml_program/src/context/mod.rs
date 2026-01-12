//! Execution context types.
//!
//! The runtime uses three context types to manage execution state:
//!
//! - `SharedCallContext` - Session-scoped context, shared across multiple calls
//! - `DynamicBamlContext` - Optional schema extensions (dynamic types, client overrides)
//! - `PerCallContext` - Per-call configuration (cancellation, env vars, tags)

mod per_call;
mod shared;

pub use baml_type_reflection::*;
pub use per_call::*;
pub use shared::*;
