#[cfg(feature = "heap_debug")]
mod real;
#[cfg(not(feature = "heap_debug"))]
mod stub;

#[cfg(feature = "heap_debug")]
pub(crate) use real::HeapDebuggerState;
#[cfg(feature = "heap_debug")]
pub use real::{HeapDebuggerConfig, HeapVerifyMode};
#[cfg(not(feature = "heap_debug"))]
pub(crate) use stub::HeapDebuggerState;
#[cfg(not(feature = "heap_debug"))]
pub use stub::{HeapDebuggerConfig, HeapVerifyMode};
