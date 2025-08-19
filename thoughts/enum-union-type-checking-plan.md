# Comprehensive Enum Union Type Checking Implementation Plan

## Problem Analysis

The current binary operation type checking in `@engine/baml-lib/jinja/src/evaluate_type/expr.rs` has several scenarios that are not correctly handled when dealing with union types containing enum values. The existing implementation only checks for direct `Type::EnumValueRef` patterns, missing complex union scenarios.

## Different Scenarios to Handle

### 1. **Simple Enum vs String** (Already handled)
- `status == "Active"` where `status: Status` 
- **Current behavior**: ✅ Generates warning
- **Desired behavior**: ✅ Continue generating warning

### 2. **Nullable Enum vs String** (Partially broken)
- `nullable_status == "Active"` where `nullable_status: Status?` (i.e., `Status | null`)
- **Current behavior**: ❌ No warning (falls through to default union handling)
- **Desired behavior**: ✅ Should generate same enum warning as non-nullable

### 3. **Union with Enum + Non-Nullish Types vs String** (Critical edge case)
- `status_or_string == "Active"` where `status_or_string: Status | string`
- **Current behavior**: ❌ May generate incorrect enum warnings
- **Desired behavior**: ❌ Should NOT generate enum warnings (legitimate comparison)

### 4. **Union with Multiple Enums vs String** (Edge case)
- `multi_enum == "Active"` where `multi_enum: Status | Priority`
- **Current behavior**: ❌ Undefined behavior
- **Desired behavior**: ❌ Should NOT generate enum warnings (ambiguous enum type)

### 5. **Nullable Enum vs Nullable Enum** (Cross-nullable comparison)
- `nullable_status == nullable_priority` where both are `Enum?`
- **Current behavior**: ❌ Falls through to generic union handling
- **Desired behavior**: ✅ Should generate cross-enum type error if different enum types

### 6. **Nullable Enum vs Regular Enum** (Mixed nullable comparison)
- `nullable_status == status` where `nullable_status: Status?` and `status: Status`
- **Current behavior**: ❌ Falls through to generic union handling  
- **Desired behavior**: ✅ Should allow comparison (compatible types)

### 7. **Complex Union Containing String vs String** (String union edge case)
- `complex_union == "value"` where `complex_union: Status | string | int`
- **Current behavior**: ❌ May generate incorrect enum warnings
- **Desired behavior**: ❌ Should NOT generate enum warnings (legitimate string comparison)

### 8. **Generic String vs Union** (Reverse scenarios)
- All of the above but with operands reversed (e.g., `"Active" == nullable_status`)
- **Current behavior**: ❌ May have different handling than forward direction
- **Desired behavior**: ✅ Should have identical handling to forward direction

## Solution Design

### Core Logic: Union Classification

Create a helper function to classify union types:

```rust
#[derive(Debug)]
enum UnionClassification<'a> {
    /// Union contains exactly one enum type + only nullish types (None, Undefined)
    NullableEnum(&'a str),
    /// Union contains exactly one enum type + non-nullish types (like string, int)
    EnumWithOtherTypes(&'a str),
    /// Union contains multiple different enum types
    MultipleEnums(Vec<&'a str>),
    /// Union contains no enum types
    NoEnums,
    /// Union contains mixed complex types that can't be classified simply
    Complex,
}

fn classify_union(types: &[Type]) -> UnionClassification {
    let mut enum_names = std::collections::HashSet::new();
    let mut has_non_nullish_non_enum = false;
    
    for t in types {
        match t {
            Type::EnumValueRef(name) => {
                enum_names.insert(name.as_str());
            }
            Type::None | Type::Undefined => {
                // Nullish types are fine in nullable enums
                continue;
            }
            Type::String | Type::Int | Type::Float | Type::Bool | Type::Number
            | Type::Literal(_) | Type::List(_) | Type::Map(_, _) => {
                has_non_nullish_non_enum = true;
            }
            Type::Union(_) | Type::Both(_, _) => {
                // Nested unions/complex types make this too complex to handle simply
                return UnionClassification::Complex;
            }
            _ => {
                // Other complex types like ClassRef, etc.
                has_non_nullish_non_enum = true;
            }
        }
    }
    
    match (enum_names.len(), has_non_nullish_non_enum) {
        (0, _) => UnionClassification::NoEnums,
        (1, false) => UnionClassification::NullableEnum(enum_names.iter().next().unwrap()),
        (1, true) => UnionClassification::EnumWithOtherTypes(enum_names.iter().next().unwrap()),
        (2.., _) => UnionClassification::MultipleEnums(enum_names.into_iter().collect()),
    }
}
```

### Enhanced Binary Operation Logic

Add new pattern matching **before** existing enum cases (around line 126 in `expr.rs`):

```rust
// Handle Union types in binary operations
match (&lhs, &rhs) {
    // Union vs String Literal - check if enum warning is appropriate
    (Type::Union(types), Type::Literal(LiteralValue::String(str_val)))
    | (Type::Literal(LiteralValue::String(str_val)), Type::Union(types)) => {
        match classify_union(types) {
            UnionClassification::NullableEnum(enum_name) => {
                // This is a nullable enum - generate enum warning
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
                        // Non-comparison operations - fall through to existing logic
                        handle_default_binop(bin_expr, &lhs, &rhs)
                    }
                }
            }
            UnionClassification::EnumWithOtherTypes(_) | UnionClassification::NoEnums => {
                // Union contains string or other types - legitimate string comparison
                // Fall through to existing logic without enum warning
                handle_default_binop(bin_expr, &lhs, &rhs)
            }
            UnionClassification::MultipleEnums(_) | UnionClassification::Complex => {
                // Too complex or ambiguous - fall through to existing logic
                handle_default_binop(bin_expr, &lhs, &rhs)
            }
        }
    }
    
    // Union vs Generic String - similar logic
    (Type::Union(types), Type::String) | (Type::String, Type::Union(types)) => {
        match classify_union(types) {
            UnionClassification::NullableEnum(enum_name) => {
                match bin_expr.op {
                    ast::BinOpKind::Eq | ast::BinOpKind::Ne 
                    | ast::BinOpKind::Lt | ast::BinOpKind::Gt 
                    | ast::BinOpKind::Lte | ast::BinOpKind::Gte => {
                        state.errors.push(TypeError::new_enum_literal_suggestion(
                            expr,
                            enum_name,
                            "lorem-ipsum-todo-placeholder", // Existing pattern for generic string
                            types,
                            expr.span(),
                        ));
                        Type::Bool
                    }
                    _ => handle_default_binop(bin_expr, &lhs, &rhs)
                }
            }
            _ => handle_default_binop(bin_expr, &lhs, &rhs)
        }
    }
    
    // Union vs Union - handle cross-nullable enum comparisons
    (Type::Union(left_types), Type::Union(right_types)) => {
        let left_class = classify_union(left_types);
        let right_class = classify_union(right_types);
        
        match (&left_class, &right_class) {
            (UnionClassification::NullableEnum(left_enum), UnionClassification::NullableEnum(right_enum)) => {
                // Both are nullable enums
                match bin_expr.op {
                    ast::BinOpKind::Eq | ast::BinOpKind::Ne 
                    | ast::BinOpKind::Lt | ast::BinOpKind::Gt 
                    | ast::BinOpKind::Lte | ast::BinOpKind::Gte => {
                        if left_enum == right_enum {
                            Type::Bool // Same enum type - valid comparison
                        } else {
                            state.errors.push(TypeError::new_invalid_enum_cmp(
                                expr,
                                &Type::EnumValueRef(left_enum.to_string()),
                                &Type::EnumValueRef(right_enum.to_string()),
                                expr.span(),
                            ));
                            Type::Bool
                        }
                    }
                    _ => handle_default_binop(bin_expr, &lhs, &rhs)
                }
            }
            _ => handle_default_binop(bin_expr, &lhs, &rhs)
        }
    }
    
    // Union vs EnumValueRef - handle mixed nullable/non-nullable
    (Type::Union(types), Type::EnumValueRef(enum_name))
    | (Type::EnumValueRef(enum_name), Type::Union(types)) => {
        match classify_union(types) {
            UnionClassification::NullableEnum(union_enum) => {
                match bin_expr.op {
                    ast::BinOpKind::Eq | ast::BinOpKind::Ne 
                    | ast::BinOpKind::Lt | ast::BinOpKind::Gt 
                    | ast::BinOpKind::Lte | ast::BinOpKind::Gte => {
                        if union_enum == enum_name {
                            Type::Bool // Compatible enum types
                        } else {
                            state.errors.push(TypeError::new_invalid_enum_cmp(
                                expr,
                                &Type::EnumValueRef(union_enum.to_string()),
                                &Type::EnumValueRef(enum_name.to_string()),
                                expr.span(),
                            ));
                            Type::Bool
                        }
                    }
                    _ => handle_default_binop(bin_expr, &lhs, &rhs)
                }
            }
            _ => handle_default_binop(bin_expr, &lhs, &rhs)
        }
    }
    
    // Existing enum cases continue unchanged...
    (Type::EnumValueRef(e1), Type::EnumValueRef(e2)) => {
        // ... existing logic
    }
    // ... other existing cases
}

fn handle_default_binop(bin_expr: &ast::BinOp, lhs: &Type, rhs: &Type) -> Type {
    // The existing default binary operation logic (lines 226-251)
    match bin_expr.op {
        ast::BinOpKind::Add => {
            if lhs.is_subtype_of(&Type::String) || rhs.is_subtype_of(&Type::String) {
                Type::String
            } else {
                Type::Number
            }
        }
        // ... rest of existing logic
    }
}
```

## Implementation Plan

### Step 1: Add Helper Functions
- Add `UnionClassification` enum and `classify_union` function
- Add `handle_default_binop` helper to avoid code duplication

### Step 2: Update Binary Operation Logic
- Insert new pattern matching cases **before** existing enum cases
- Ensure proper fallback to existing logic for non-enum scenarios

### Step 3: Update Tests
Add test cases to `/Users/sam/baml2/engine/baml-lib/baml/tests/validation_files/enum/enum_jinja_syntax_validation.baml`:

```baml
// Add after line 12, new function parameters:
function TestEnumUnionSyntax(
    status: Status,
    priority: Priority, 
    nullable_status: Status?,
    status_or_string: Status | string,
    multi_enum: Status | Priority
) -> string {
    // ... test cases for each scenario
}
```

### Step 4: Expected Warnings
- **Should generate warnings**: Lines 132, 139 (nullable enum comparisons)
- **Should NOT generate warnings**: New test cases for `Status | string` scenarios
- **Should generate cross-enum errors**: Mixed enum union comparisons

## Test Scenarios Matrix

| Left Type | Right Type | Comparison | Expected Behavior |
|-----------|------------|------------|-------------------|
| `Status?` | `"Active"` | `==` | ✅ Enum warning |
| `Status | string` | `"Active"` | `==` | ❌ No warning (legitimate) |
| `Status | Priority` | `"Active"` | `==` | ❌ No warning (ambiguous) |
| `Status?` | `Priority?` | `==` | ❌ Cross-enum error |
| `Status?` | `Status` | `==` | ✅ Valid comparison |
| `Status | string | int` | `"Active"` | `==` | ❌ No warning (complex) |

## Benefits of This Solution

1. **Precision**: Only generates enum warnings for true nullable enums (`Enum | null`), not mixed unions
2. **Completeness**: Handles all union scenarios including complex nested cases
3. **Consistency**: Same logic for both directions of comparison (`A == B` and `B == A`)
4. **Backward Compatibility**: Doesn't change existing non-union enum handling
5. **Future-Proof**: Extensible classification system for other union scenarios

## Risk Assessment

- **Low Risk**: Changes are additive, existing logic remains unchanged
- **Medium Complexity**: Requires careful testing of union classification edge cases
- **High Value**: Fixes current false negatives (missing warnings) and prevents false positives

## Files to Modify

1. `/Users/sam/baml2/engine/baml-lib/jinja/src/evaluate_type/expr.rs` - Main implementation
2. `/Users/sam/baml2/engine/baml-lib/baml/tests/validation_files/enum/enum_jinja_syntax_validation.baml` - Test cases
3. Potentially create new test file for union-specific scenarios if current file becomes too large