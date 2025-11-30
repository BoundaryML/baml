# Deep Dive: Rust Error Handling

## Core Philosophy: Type-System Enforced Handling
Rust uses the `Result<T, E>` enum (Sum Type) to represent success or failure. This forces the caller to handle the error (or explicitly ignore it) at compile time.

```rust
enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

## Developer Experience (DX)

### Ergonomics with `?`
Rust combines explicit values with ergonomic propagation. The `?` operator is syntax sugar for "unwrap or return early".

```rust
fn read_config() -> Result<Config, io::Error> {
    let mut file = File::open("config.toml")?; // Returns Err(io::Error) immediately if failed
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(parse(contents))
}
```

**DX Pros**:

- **Concise**: `?` removes the `if err != nil` boilerplate while keeping the "early return" semantics visible.
- **Chaining**: Works well with functional combinators: `File::open(path).and_then(|f| ...)`

**DX Cons**:

- **Type Alignment**: `?` only works if the error type matches the function's return error type (or can be converted via `From`). This leads to "error soup" where you need a common error enum or a boxed trait object.

### The Ecosystem: `thiserror` vs `anyhow`
Rust's error handling splits into two main use cases:

1.  **Libraries (`thiserror`)**:
    Libraries must export precise error types so callers can handle specific cases. `thiserror` derives the `Error` trait for enums.
    ```rust
    #[derive(thiserror::Error, Debug)]
    pub enum DataStoreError {
        #[error("data store disconnected")]
        Disconnect(#[from] io::Error),
        #[error("the data for key `{0}` is not available")]
        Redaction(String),
    }
    ```

2.  **Applications (`anyhow`)**:
    Apps usually just want to report errors. `anyhow::Result<T>` is a wrapper around `Box<dyn Error + Send + Sync>` with easy context attachment.
    ```rust
    fn main() -> anyhow::Result<()> {
        let config = read_config().context("failed to read configuration")?;
        Ok(())
    }
    ```

## Implementation Tradeoffs

### 1. Zero-Cost Abstractions vs. Binary Size
**Tradeoff**: `Result` is a value.

- **Benefit**: **Zero runtime overhead** for the "happy path" (unlike try-catch setup costs in some languages). The compiler optimizes `Result` layout (e.g., `Option<Box<T>>` is a single pointer, null=None).
- **Cost**: **Monomorphization**. Generic functions using `Result` generate code for every `T` and `E`, potentially increasing binary size.

### 2. Recoverable vs. Unrecoverable
Rust strictly separates:

- **`Result`**: Recoverable errors (file not found, network timeout).
- **`panic!`**: Unrecoverable bugs (index out of bounds). Unwinds the stack (or aborts).
- **Tradeoff**: Forces developers to categorize errors. You can't just "catch everything" including panics easily (requires `catch_unwind`, which is discouraged for logic control).

### 3. Exhaustiveness
**Tradeoff**: `match` must cover all cases.

- **Benefit**: Refactoring safety. Adding a new error variant breaks the build, forcing you to handle it.
- **Cost**: Boilerplate when you just want to pass it up. (Solved mostly by `?`).

## Summary
Rust provides the **safety of checked exceptions** with the **ergonomics of unchecked exceptions** (via `?`) and the **performance of C return codes**. The learning curve lies in managing error types and conversions.
