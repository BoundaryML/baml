# Error Handling: A Cross-Language Survey

This document serves as an index for deep-dive research into error handling mechanisms across modern programming languages. It focuses on **Developer Experience (DX)**, **Implementation Tradeoffs**, and **Core Philosophies**.

## Detailed Language Deep Dives

*   **[Go](./go.md)**
    *   **Philosophy**: Errors are Values. Explicit control flow.
    *   **Key Insight**: Optimizes for local reasoning at the cost of verbosity (`if err != nil`).
*   **[Rust](./rust.md)**
    *   **Philosophy**: Type-System Enforced Handling (`Result<T, E>`).
    *   **Key Insight**: Zero-cost abstractions with ergonomic propagation (`?`).
*   **[Swift](./swift.md)**
    *   **Philosophy**: Hybrid. Explicit control flow syntax (`try`) with efficient value-based implementation.
    *   **Key Insight**: "Typed Throws" (Swift 6) bridges the gap between checked exceptions and result types.
*   **[Python](./python.md)**
    *   **Philosophy**: EAFP (Easier to Ask for Forgiveness). Exceptions as standard control flow.
    *   **Key Insight**: Optimizes for rapid development; accepts runtime overhead for raised exceptions.
*   **[TypeScript](./typescript.md)**
    *   **Philosophy**: Dynamic & Permissive.
    *   **Key Insight**: The friction between static types and runtime reality leads many to adopt library-based `Result` patterns.
*   **[Java](./java.md)**
    *   **Philosophy**: Checked Exceptions (Compile-time safety).
    *   **Key Insight**: The "Lambda Problem" and API versioning issues have pushed the ecosystem toward Unchecked Exceptions.

## Comparative Analysis

### 1. Control Flow vs. Values
| Language | Mechanism | Propagation | DX Note |
| :--- | :--- | :--- | :--- |
| **Go** | Values (`error`) | Explicit return | Verbose but obvious. |
| **Rust** | Values (`Result`) | `?` operator | Concise, explicit, type-safe. |
| **Swift** | Syntax (`throw`) | `try` keyword | Looks like exceptions, acts like values. |
| **Python** | Exceptions | Implicit | Clean "happy path", requires docs. |
| **TypeScript** | Exceptions | Implicit | No static guarantees; community uses `Result` types. |
| **Java** | Exceptions | Implicit (Unchecked) / Explicit (Checked) | Checked can be burdensome. |

### 2. Type Safety

*   **Strict (Sum Types)**: Rust, Swift (Typed). You know exactly what errors are possible.
*   **Nominal (Inheritance)**: Java, Python. You catch by class hierarchy. Good for broad categories, bad for precision.
*   **Loose**: TypeScript, Go (Interface). You often need runtime checks (`instanceof`, `errors.As`) to know what happened.

### 3. Performance

*   **Zero Cost (Happy Path)**: Rust, C++, Swift. No overhead if no error.
*   **Allocation Heavy**: Java, Python. Creating exceptions usually captures stack traces, which is slow.
*   **Value Heavy**: Go. Errors are small values, cheap to create, but no stack traces by default.
