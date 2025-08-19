# Implementation Plan: Fix Lorem-Ipsum-Todo-Placeholder

## Problem Statement

Currently, when comparing an enum to a generic `Type::String` (not a specific string literal), the type checker uses a placeholder string `"lorem-ipsum-todo-placeholder"` in the warning generation. This creates awkward and unhelpful warning messages.

## Solution: Generic Enum-String Comparison Warning with GitHub Issue Reference

### 1. Add New Method to TypeError
**Location**: `/Users/sam/baml2/engine/baml-lib/jinja/src/evaluate_type/mod.rs`

Add new method after the existing error methods (around line 340):

```rust
fn new_enum_string_cmp_deprecated(
    expr: &Expr,
    enum_name: &str,
    span: Span,
) -> Self {
    Self {
        message: format!(
            "Comparing enum {} to string variable - enum-string comparisons will soon be deprecated. Please see https://github.com/BoundaryML/baml/issues/2339.",
            enum_name
        ),
        span,
    }
}
```

### 2. Replace Placeholder Calls in Binary Operations
**Location**: `/Users/sam/baml2/engine/baml-lib/jinja/src/evaluate_type/expr.rs`

Find the two locations using `lorem-ipsum-todo-placeholder` (around lines 193-200):

**Replace:**
```rust
state.errors.push(TypeError::new_enum_literal_suggestion(
    expr,
    enum_name,
    "lorem-ipsum-todo-placeholder",
    types,
    expr.span(),
));
```

**With:**
```rust
state.errors.push(TypeError::new_enum_string_cmp_deprecated(
    expr,
    enum_name,
    expr.span(),
));
```

### 3. Expected Message Format

```
Comparing enum Status to string variable - enum-string comparisons will soon be deprecated. Please see https://github.com/BoundaryML/baml/issues/2339.
```

### 4. Benefits

- **Removes placeholder**: No more "lorem-ipsum-todo-placeholder" 
- **References documentation**: Directs users to GitHub issue for detailed guidance
- **Consistent deprecation messaging**: Aligns with overall enum-string deprecation theme
- **Generic solution**: Works for any enum-string variable comparison

### 5. Testing

After implementation, verify that:
1. Generic enum-string comparisons generate the new warning
2. The placeholder no longer appears in any warning messages
3. Specific string literal comparisons continue to use existing suggestion logic

This approach leverages the GitHub issue as the single source of truth for enum-string comparison migration guidance.