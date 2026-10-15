//! The telemetry API without the `telemetry` feature: the instrumentation
//! the backend emits compiles to nothing, and nothing is recorded.

use std::path::PathBuf;

use crate::Thrown;

/// See the `telemetry` feature's `Function`.
#[derive(Debug)]
pub struct Function {
    pub fqn: &'static str,
    pub observed: bool,
    pub file: &'static str,
    pub file_id: u32,
    pub start: u32,
    pub end: u32,
    pub sites: &'static [Site],
}

/// See the `telemetry` feature's `Site`.
#[derive(Debug)]
pub struct Site {
    pub start: u32,
    pub end: u32,
    pub line: u32,
}

/// See the `telemetry` feature's `Destination`.
#[derive(Debug, Clone)]
pub enum Destination {
    Directory(PathBuf),
    UserFiles,
}

/// Nothing to start: the program was built without telemetry.
pub fn start(
    _functions: &'static [Function],
    _destination: Destination,
) -> Result<&'static str, String> {
    Ok("off")
}

pub fn finish() -> Result<(), String> {
    Ok(())
}

#[inline]
pub fn site(_caller: u32, _site: u32) {}

#[inline]
pub fn root<R>(call: impl FnOnce() -> Result<R, Thrown>) -> Result<R, Thrown> {
    call()
}

/// An unrecorded call.
#[must_use = "an observed call records nothing until `exit`"]
pub struct Frame;

impl Frame {
    #[inline]
    pub fn exit<T>(self, _result: &Result<T, Thrown>) {}
}

#[inline]
pub fn enter(_callee: u32) -> Frame {
    Frame
}
