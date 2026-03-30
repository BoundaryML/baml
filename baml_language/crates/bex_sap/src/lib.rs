pub use crate::jsonish::parse;

pub mod baml_value;
pub mod deserializer;
pub mod jsonish;
pub mod sap_model;
#[cfg(test)]
mod tests;
pub mod to_external;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingMode {
    NonStreaming,
    Streaming,
}
