# Implementation Plan: Nullable Enum Type Checking

## Problem Statement

Nullable enum comparisons like `nullable_status == "Active"` where `nullable_status: Status?` do not generate proper type checking warnings. The type system represents nullable enums as `Type::Union(vec![Type::EnumValueRef("Status"), Type::None])`, but the comparison logic only handles direct `Type::EnumValueRef` patterns.

## Critical Issue with Simple Union Matching

The naive approach of checking if a union contains an enum would **incorrectly handle** types like:
- `Status | string` compared to `"Active"` - Should NOT generate enum warning
- `Status | int` compared to `"Active"` - Should NOT generate enum warning  
- `Status?` (i.e., `Status | null`) compared to `"Active"` - SHOULD generate enum warning

## Solution: Check Union is ONLY Enum + Nullish Types

We need to verify that the union contains **only** an enum and nullish types (`None`, `Undefined`), not other types like `string`.

## Implementation Details

### File to Modify
`/Users/sam/baml2/engine/baml-lib/jinja/src/evaluate_type/expr.rs`

### Key Insight: Union Must Be "Nullable Enum" Pattern

A union should trigger enum warnings ONLY if it contains:
1. Exactly ONE `EnumValueRef`
2. Zero or more nullish types (`None`, `Undefined`)
3. NO other types (like `String`, `Int`, etc.)

### Implementation Approach

Add new pattern matching cases **before** the existing enum cases (around line 126):

```rust
// Helper function to check if union is nullable enum (enum + nullish only)
fn extract_enum_from_nullable_union(types: &[Type]) -> Option<&str> {
    let mut enum_name: Option<&str> = None;
    
    for t in types {
        match t {
            Type::EnumValueRef(name) => {
                if enum_name.is_some() {
                    // Multiple different enums in union - not a simple nullable enum
                    return None;
                }
                enum_name = Some(name);
            }
            Type::None | Type::Undefined => {
                // Nullish types are allowed in nullable enums
                continue;
            }
            _ => {
                // Any other type (String, Int, etc.) means this isn't a nullable enum
                return None;
            }
        }
    }
    
    enum_name
}

// In the BinOp match statement, add before existing enum cases:

// Handle nullable enum comparisons - Union containing ONLY enum + nullish
(Type::Union(types), Type::Literal(LiteralValue::String(str_val)))
| (Type::Literal(LiteralValue::String(str_val)), Type::Union(types)) => {
    // Check if this is a nullable enum (not enum | string or other union)
    if let Some(enum_name) = extract_enum_from_nullable_union(types) {
        match bin_expr.op {
            ast::BinOpKind::Eq | ast::BinOpKind::Ne 
            | ast::BinOpKind::Lt | ast::BinOpKind::Gt 
            | ast::BinOpKind::Lte | ast::BinOpKind::Gte => {
                state.errors.push(TypeError::new_enum_literal_suggestion(
                    expr,
                    enum_name,
                    str_val,
                    types,
                    expr.span(),
                ));
                Type::Bool
            }
            _ => {
                // Non-comparison operators - fall through to existing logic
            }
        }
    } else {
        // Not a nullable enum - fall through to existing union logic
    }
}

// Handle nullable-to-nullable enum comparisons
(Type::Union(left_types), Type::Union(right_types)) => {
    let left_enum = extract_enum_from_nullable_union(left_types);
    let right_enum = extract_enum_from_nullable_union(right_types);
    
    match (left_enum, right_enum) {
        (Some(left), Some(right)) => {
            // Both are nullable enums - apply enum comparison logic
            match bin_expr.op {
                ast::BinOpKind::Eq | ast::BinOpKind::Ne 
                | ast::BinOpKind::Lt | ast::BinOpKind::Gt 
                | ast::BinOpKind::Lte | ast::BinOpKind::Gte => {
                    if left == right {
                        Type::Bool
                    } else {
                        state.errors.push(TypeError::new_invalid_enum_cmp(
                            expr,
                            &Type::EnumValueRef(left.to_string()),
                            &Type::EnumValueRef(right.to_string()),
                            expr.span(),
                        ));
                        Type::Bool
                    }
                }
                _ => {
                    // Non-comparison operators - fall through
                }
            }
        }
        (Some(enum_name), None) | (None, Some(enum_name)) => {
            // One side is nullable enum, other is different union
            // Check if comparing to string literal
            if matches!((&left_types, &right_types), 
                       (types, _) | (_, types) if types.iter().any(|t| matches!(t, Type::String))) {
                // Union contains string - this is enum|string vs string case
                // Fall through to normal union handling
            } else {
                // Could still generate enum warning if appropriate
            }
        }
        _ => {
            // Neither is a nullable enum - fall through to existing union logic
        }
    }
}
```

### Handling Generic String Comparisons

For comparisons with generic `Type::String` (not literal):

```rust
// Handle nullable enum vs generic string
(Type::Union(types), Type::String) | (Type::String, Type::Union(types)) => {
    if let Some(enum_name) = extract_enum_from_nullable_union(types) {
        match bin_expr.op {
            ast::BinOpKind::Eq | ast::BinOpKind::Ne 
            | ast::BinOpKind::Lt | ast::BinOpKind::Gt 
            | ast::BinOpKind::Lte | ast::BinOpKind::Gte => {
                state.errors.push(TypeError::new_enum_literal_suggestion(
                    expr,
                    enum_name,
                    "<value>",
                    types,
                    expr.span(),
                ));
                Type::Bool
            }
            _ => {
                // Fall through
            }
        }
    } else {
        // Not a nullable enum - fall through
    }
}
```

## Test Cases to Verify

### Should Generate Enum Warnings

1. `nullable_status == "Active"` where `nullable_status: Status?`
2. `nullable_status == "active"` (alias comparison)
3. `"Pending" == nullable_status`
4. `nullable_status != "Inactive"`
5. `nullable_status > "AAA"` (ordering comparisons)
6. `nullable_priority == "High"` where `nullable_priority: Priority?`
7. `nullable_status <= "Zzz"` (less-than-equal comparison)
8. `nullable_status >= "Active"` (greater-than-equal comparison)
9. Nested nullable enum comparisons: `(nullable_status == "Active") and (nullable_priority != "Low")`
10. String concatenation patterns: `nullable_status + "_suffix"` (if we want to warn on this)

### Nullable Enum Cross-Comparisons

11. `nullable_status1 == nullable_status2` where both are `Status?`
12. `nullable_status == nullable_priority` where types are `Status?` vs `Priority?` (should error)
13. `nullable_status != nullable_priority` (cross-enum comparison error)

### Edge Cases for Nullable Enums

14. `null == nullable_status` (null comparison)
15. `nullable_status == null` (null comparison reverse)
16. `nullable_status and nullable_status == "Active"` (safe navigation pattern)
17. `nullable_status or "Default"` (fallback pattern)

### Should NOT Generate Enum Warnings

1. `status_or_string == "Active"` where `status_or_string: Status | string`
2. `mixed_type == "Something"` where `mixed_type: Status | int | string`
3. `string_var == "Active"` where `string_var: string`
4. `enum_with_string == "Active"` where `enum_with_string: Status | string | null`
5. `complex_union == "Test"` where `complex_union: Status | Priority | string`
6. `nullable_string == "Value"` where `nullable_string: string?`
7. `int_or_enum == "123"` where `int_or_enum: int | Status`

## Test File Updates

### File to Update
`/Users/sam/baml2/engine/baml-lib/baml/tests/validation_files/enum/enum_jinja_syntax_validation.baml`

### Add Expected Warnings

For nullable enum comparisons around lines 127 and 134:

```
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_jinja_syntax_validation.baml:127
//    | 
// 126 |             No status provided
// 127 |         {% elif nullable_status == "Active" %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_jinja_syntax_validation.baml:134
//    | 
// 133 |         // Safe navigation pattern
// 134 |         {% if nullable_status and nullable_status == "Pending" %}
//    | 
// warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_jinja_syntax_validation.baml:68
//    | 
// 67 |         // Complex boolean expressions
// 68 |         {% if (status == "Active" or priority == "High") and nullable_status != "Inactive" %}
//    |
```

## Verification Steps

1. Test nullable enum comparisons generate warnings:
   ```bash
   cargo test -p baml-lib --test validation_tests enum_jinja_syntax_validation
   ```

2. Create additional test to verify `Status | string` does NOT generate enum warnings

3. Verify cross-nullable-enum comparisons work correctly

## Benefits

1. **Correct Semantics**: Only treats `Enum | null` as nullable enum, not `Enum | string`
2. **Complete Coverage**: Handles all nullable enum comparison scenarios
3. **Maintains Compatibility**: Doesn't break existing union type checking
4. **Clear Diagnostics**: Same helpful warnings for nullable and non-nullable enums

## Risk Assessment

- **Medium Complexity**: Need careful logic to distinguish nullable enums from other unions
- **Testing Required**: Must verify doesn't affect `Enum | string` or other union types
- **Performance**: Minimal impact - only adds one helper function and pattern matches