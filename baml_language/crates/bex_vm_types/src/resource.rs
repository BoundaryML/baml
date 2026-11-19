//! The operations of each resource kind: state a platform owns (an open file,
//! a socket), which a `@@rust_type` class reaches only through the trait its
//! kind declares. Every platform implements the trait on its own handle, so
//! nothing downcasts.
//!
//! Generated from the `$rust_io_function` methods of the tagged classes in
//! `baml_builtins2`, with the plain data their signatures need.

include!(concat!(env!("OUT_DIR"), "/resource_generated.rs"));
