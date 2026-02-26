pub mod baml_value;
pub mod deserializer;
mod helpers;
pub mod jsonish;
pub mod sap_model;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingMode {
    NonStreaming,
    Streaming,
}
