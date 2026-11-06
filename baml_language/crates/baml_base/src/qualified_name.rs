//! Canonical fully qualified names shared across the compiler, runtime, and
//! SDK generators.

/// FQN (as a display string) of the recursive `json` type alias defined in
/// `baml_builtins2/baml_std/baml/ns_json/json.baml`.
///
/// Used as a sentinel across Phases 3 and 6 to short-circuit alias-body
/// recursion for output-format rendering, SAP streaming, and Python codegen.
pub const BAML_JSON_JSON: &str = "baml.json.json";

/// Canonical host-facing typed AI stream. Compiler synthesis, engine handle
/// lifting, and SDK generators must share this identity rather than carrying
/// independent string literals.
pub const AI_STREAM_STREAM: &str = "ai.stream.Stream";

/// Canonical bound LLM function specification capability.
pub const AI_FUNCTION_SPEC: &str = "ai.FunctionSpec";

/// Canonical nominal end-of-stream marker used by AI streams.
pub const AI_STREAM_DONE: &str = "ai.stream.Done";
