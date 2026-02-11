#[cfg(feature = "heap_debug")]
mod real;
#[cfg(not(feature = "heap_debug"))]
mod stub;

#[cfg(feature = "heap_debug")]
pub use real::HeapDebuggerConfig;
#[cfg(feature = "heap_debug")]
pub(crate) use real::HeapDebuggerState;
#[allow(unused_imports)]
#[cfg(feature = "heap_debug")]
pub use real::HeapVerifyMode;
#[cfg(not(feature = "heap_debug"))]
pub use stub::HeapDebuggerConfig;
#[cfg(not(feature = "heap_debug"))]
pub(crate) use stub::HeapDebuggerState;
#[allow(unused_imports)]
#[cfg(not(feature = "heap_debug"))]
pub use stub::HeapVerifyMode;
