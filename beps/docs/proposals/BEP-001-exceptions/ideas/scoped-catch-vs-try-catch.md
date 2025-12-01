# Scoped Catch vs. Traditional Try-Catch

This document compares BAML's **Scoped Catch** syntax with the traditional `try-catch` blocks found in languages like Java, TypeScript, and Python.

## The Core Difference

### Traditional Try-Catch (Wrapping)
In traditional languages, error handling is a **wrapper** around code. You must indent the "happy path" inside a block.

```typescript
// TypeScript
function extract(text: string) {
    try {
        // 1. Indentation level increases
        // 2. Scope of variables declared here is limited to the try block
        const client = new Client();
        return client.run(text);
    } catch (e) {
        // 3. Handler is at the bottom, far from the start
        return fallback();
    }
}
```

### BAML Scoped Catch (Header)
In BAML, error handling is a **header** for the scope. You declare it at the top, and it applies to everything below.

```baml
// BAML
function Extract(text: string) {
    // 1. No indentation change for happy path
    catch {
        error => { return fallback() }
    }

    // 2. Variables declared here are visible to the whole function
    client "openai"
    return prompt(text)
}
```

## Comparison Dimensions

### 1. Diff Size & Code Evolution
**Scenario**: You have a working prototype and want to add error handling.

- **Try-Catch**: Requires a **large diff**. You must wrap existing lines in `try { ... }`, changing indentation for every line. In git, this looks like you rewrote the whole function.
- **Scoped Catch**: Requires a **minimal diff**. You insert 3 lines at the top. The rest of the file is untouched.
    - *Why this matters for AI*: AI agents (and humans) are less likely to make mistakes when changes are additive rather than structural.

### 2. Variable Scoping
**Scenario**: You want to access a variable declared in the "try" block after the block ends (if no error occurred).

- **Try-Catch**: Variables declared inside `try` are not visible outside. You must declare them *before* the try block (hoisting).
    ```typescript
    let result; // Hoisting required
    try {
        result = complexOperation();
    } catch (e) { ... }
    use(result);
    ```
- **Scoped Catch**: Variables declared in the scope are visible naturally because there is no inner block.
    ```baml
    catch { ... }
    let result = complexOperation() // No hoisting needed
    use(result)
    ```

### 3. Readability & Mental Model
- **Try-Catch**: "Attempt this block of code, and if it fails, jump down here."
    - *Pro*: Explicit boundary of what is covered.
    - *Con*: Separates the "what we are doing" from "how we handle failure" by a potentially large block of code.
- **Scoped Catch**: "For this entire scope, here is the failure policy."
    - *Pro*: Acts as a declarative header. You see the failure policy before you see the logic.
    - *Con*: Implicit boundary (end of scope).

### 4. Refactoring Friction
- **Try-Catch**: Moving code in/out of the `try` block requires re-indenting.
- **Scoped Catch**: Moving code in/out of the scope is just cut-paste, no re-indenting of the code itself (though the catch block stays at the top).

## Why BAML Chose Scoped Catch

BAML is designed for **AI Engineers** who often move from "prompt engineering" (prototyping) to "production engineering" (hardening).

1.  **Prototyping First**: Users start with simple, linear scripts.
2.  **Additive Hardening**: We want users to "sprinkle" reliability onto their scripts without rewriting them.
3.  **Agentic Workflow**: We expect AI agents to write and modify BAML code. Additive syntax is safer for LLMs to generate than structural refactors.

## Summary Table

| Feature | Traditional Try-Catch | BAML Scoped Catch |
| :--- | :--- | :--- |
| **Syntax Type** | Wrapper (Block) | Header (Statement) |
| **Indentation** | Increases for happy path | Unchanged |
| **Variable Scope** | Limited to try-block | Function/Scope-wide |
| **Diff Size** | Large (structural change) | Small (additive change) |
| **Placement** | Around risky code | Top of scope |
| **Mental Model** | "Attempt this specific part" | "Policy for this scope" |
