# BAML Runtime

This is the core interface for using BAML files.

General interface is:

```rust
struct BamlRuntime {
  // Opaque
}

struct FunctionResponse {
  id: UUID
  result: Option<serde::Value>
  llm_response: Option<LLMResponse>
}

enum FunctionResponseStatus {
  Passed(serde::Value)
  Failed(Option<&str>, Option<serde::Value>)
}

struct LLMResponse {
  message: String;
  model: String;
  metadata: HashMap<&str, serde::Value>;
}

impl BamlRuntime  {
  // Not available in WASM
  fn create_from_directory(path: Path) -> Result<BamlRuntime>;

  fn create_from_files(files: Path[]) -> Result<BamlRuntime>;

  // Awaitable
  fn run_function(function_name: &str, params: HashMap<&str, serde::Value>) -> Result<FunctionResponse>

  // Awaitable
  fn stream_function(function_name: &str, params: HashMap<&str, serde::Value>) -> Result<...>
}
```
