//! Execution context types.
//!
//! The runtime uses three context types to manage execution state:
//!
//! - `SharedCallContext` - Session-scoped context, shared across multiple calls
//! - `DynamicBamlContext` - Optional schema extensions (dynamic types, client overrides)
//! - `PerCallContext` - Per-call configuration (cancellation, env vars, tags)

mod dynamic;
mod per_call;
mod shared;

pub use dynamic::*;
pub use per_call::*;
pub use shared::*;
