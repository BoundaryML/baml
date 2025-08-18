# Research: Enabling Enum-to-String Comparisons in Mini Jinja

## Executive Summary
This document outlines the research into enabling enum-to-string comparisons in Mini Jinja templates within the BAML codebase. Currently, comparisons like `{% if enum_arg == "VALUE_A" %}` fail with a type mismatch error. This research explores multiple approaches to enable this functionality.

## Current Implementation

### File Locations
- **Main Implementation**: `/Users/vbv/repos/baml-2/engine/baml-lib/jinja-runtime/src/baml_value_to_jinja_value.rs`
- **Type Checking**: `/Users/vbv/repos/baml-2/engine/baml-lib/jinja/src/evaluate_type/expr.rs`
- **Runtime**: `/Users/vbv/repos/baml-2/engine/baml-lib/jinja-runtime/src/lib.rs`

### Current Behavior
The `MinijinjaBamlEnumValue` struct implements the `custom_cmp` method (lines 206-217) that only compares with other enum values:

```rust
fn custom_cmp(
    self: &Arc<Self>,
    other: &minijinja::value::DynObject,
) -> Option<std::cmp::Ordering> {
    let other = other.downcast_ref::<Self>()?;  // Only works with same type
    Some(
        self.value
            .cmp(&other.value)
            .then(self.alias.cmp(&other.alias)),
    )
}
```

### Test Evidence
From `/Users/vbv/repos/baml-2/engine/baml-lib/jinja/src/evaluate_type/test_expr.rs`:
```rust
assert_fails_to!("enum_arg == \"VALUE_A\"", &types),
vec!["Type mismatch: '(enum_arg == VALUE_A)' compares values of different types (enum VALUE_A and literal[\"VALUE_A\"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `\"VALUE_A\"`)."]
```

## Technical Challenges

### 1. Mini Jinja API Limitations
- **DynObject vs Value**: The `custom_cmp` method receives a `&DynObject` parameter, not a `&Value`
- **No String Extraction**: Cannot directly check if the comparison is against a string from `DynObject`
- **Type-Safe Downcasting**: `downcast_ref::<Self>()` only works for same-type comparisons

### 2. Design Philosophy
- BAML explicitly blocks implicit type conversions for type safety
- The error message indicates this was a deliberate design decision in v0.206.0

## Proposed Solutions

### Solution 1: Custom Filter Approach ⭐ **RECOMMENDED - EASIEST**

**Implementation Location**: `/Users/vbv/repos/baml-2/engine/baml-lib/baml-core/src/ir/jinja_helpers.rs`

```rust
// In get_env() function around line 32-34
pub fn get_env() -> minijinja::Environment<'static> {
    let mut env = minijinja::Environment::new();
    
    // Add custom filter for enum to string conversion
    env.add_filter("as_string", enum_to_string_filter);
    
    // ... existing filters
    env
}

fn enum_to_string_filter(value: minijinja::Value) -> Result<String, minijinja::Error> {
    Ok(value.to_string())
}
```

**Usage in Templates**:
```jinja
{% if enum_arg|as_string == "VALUE_A" %}
    Enum matches string VALUE_A
{% endif %}
```

**Pros**:
- Simple to implement
- Follows Jinja's filter philosophy
- No Mini Jinja modifications needed
- Explicit type conversion (good for clarity)

**Cons**:
- Requires users to remember to use the filter

### Solution 2: Add Properties to Enum Objects ⭐ **MOST INTUITIVE**

**Implementation Location**: `/Users/vbv/repos/baml-2/engine/baml-lib/jinja-runtime/src/baml_value_to_jinja_value.rs` (line 194-196)

```rust
impl Object for MinijinjaBamlEnumValue {
    // ... existing methods ...
    
    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        match key.as_str()? {
            "value" => Some(minijinja::Value::from(self.value.clone())),
            "alias" => self.alias.as_ref().map(|a| minijinja::Value::from(a.clone())),
            "name" => Some(minijinja::Value::from(
                self.alias.as_ref().unwrap_or(&self.value).clone()
            )),
            _ => None,
        }
    }
}
```

**Usage in Templates**:
```jinja
{% if enum_arg.value == "VALUE_A" %}
    Enum value matches string
{% endif %}

{% if enum_arg.name == "ALIAS_A" %}
    Enum alias matches string
{% endif %}
```

**Pros**:
- Intuitive property access
- Allows access to both value and alias
- No Mini Jinja modifications needed
- Discoverable through property enumeration

**Cons**:
- Slightly more verbose than direct comparison

### Solution 3: Global Helper Function

**Implementation Location**: `/Users/vbv/repos/baml-2/engine/baml-lib/jinja-runtime/src/lib.rs` (around line 130)

```rust
// In render_minijinja function
env.add_global("str", minijinja::Value::from_function(
    |value: minijinja::Value| -> Result<String, minijinja::Error> {
        Ok(value.to_string())
    }
));
```

**Usage in Templates**:
```jinja
{% if str(enum_arg) == "VALUE_A" %}
    Enum as string matches
{% endif %}
```

**Pros**:
- Familiar to Python users
- Can be used for any type conversion
- Simple implementation

**Cons**:
- Global namespace pollution
- Less discoverable than filters

### Solution 4: Enhanced custom_cmp (REQUIRES MINI JINJA CHANGES)

This approach would require modifying Mini Jinja itself to support cross-type comparisons.

**Theoretical Implementation**:
```rust
fn custom_cmp(
    self: &Arc<Self>,
    other: &minijinja::Value,  // Changed from DynObject
) -> Option<std::cmp::Ordering> {
    // Try enum comparison first
    if let Some(other_enum) = other.downcast_object_ref::<Self>() {
        return Some(
            self.value.cmp(&other_enum.value)
                .then(self.alias.cmp(&other_enum.alias))
        );
    }
    
    // Try string comparison
    if let Some(other_str) = other.as_str() {
        return Some(self.value.cmp(&other_str.to_string()));
    }
    
    None
}
```

**Pros**:
- Most seamless user experience
- Direct comparison syntax

**Cons**:
- Requires forking/modifying Mini Jinja
- Maintenance burden
- Goes against BAML's type safety philosophy

## Implementation Complexity Analysis

| Solution | Implementation Effort | User Experience | Type Safety | Maintenance |
|----------|---------------------|-----------------|-------------|-------------|
| Custom Filter | Low (30 min) | Good | High | Low |
| Properties | Low (1 hour) | Excellent | High | Low |
| Global Function | Low (30 min) | Good | High | Low |
| Enhanced custom_cmp | High (days) | Excellent | Medium | High |

## Recommendation

**Implement both Solution 1 (Custom Filter) and Solution 2 (Properties)** for maximum flexibility:

1. **Custom Filter** provides a quick way to convert any value to string
2. **Properties** provide intuitive access to enum internals

This combination gives users multiple ways to achieve their goal without compromising type safety or requiring Mini Jinja modifications.

## Implementation Steps

### Phase 1: Custom Filter (Quick Win)
1. Add `as_string` filter to `jinja_helpers.rs`
2. Update tests to verify filter works
3. Document in user guide

### Phase 2: Enum Properties
1. Modify `MinijinjaBamlEnumValue::get_value` method
2. Add `.value`, `.alias`, and `.name` properties
3. Update type checking in `evaluate_type/expr.rs`
4. Add comprehensive tests
5. Update documentation

### Phase 3: Validation
1. Ensure existing tests still pass
2. Add new test cases for both approaches
3. Update error messages if needed
4. Performance testing

## Alternative Considerations

### Why Not Implicit Conversion?
BAML made a deliberate choice in v0.206.0 to prevent implicit string-to-enum conversions for type safety. This decision should be respected as it prevents subtle bugs and makes code more explicit.

### Future Mini Jinja Enhancement
Consider proposing an enhancement to Mini Jinja to support a new comparison method that receives `&Value` instead of `&DynObject`, enabling more flexible cross-type comparisons.

## Code References

### Key Files Modified in Your Branch
- `engine/baml-lib/jinja-runtime/src/baml_value_to_jinja_value.rs` - Enum value implementation
- `engine/baml-lib/jinja-runtime/src/lib.rs` - Runtime rendering logic
- `engine/baml-lib/jinja/src/evaluate_type/expr.rs` - Type checking for comparisons
- `engine/baml-lib/parser-database/src/walkers/enum.rs` - Enum walker implementation

### Test Files to Update
- `engine/baml-lib/jinja/src/evaluate_type/test_expr.rs` - Type checking tests
- `engine/baml-lib/jinja-runtime/src/lib.rs` (test module) - Runtime tests

## Validation Error Analysis (Added 2025-08-18)

### Current Issues with Error Messages

Based on analysis of the git diff and validation test files, several error messages need improvement:

#### 1. False "Expected Class" Warnings for Enum Properties

**Location**: `engine/baml-lib/jinja/src/evaluate_type/expr.rs:402-424`

**Problem**: When accessing enum properties like `.value`, `.alias`, or `.display`, the validator incorrectly expects a "class" type instead of recognizing these as valid enum properties.

**Example Error**:
```
warning: 'priority' is a enum Priority, expected class
  -->  enum/enum_comparison_edge_cases.baml:54
   | 
53 |         // Test 7: Using properties
54 |         Priority value: {{ priority.value }}
   |
```

**Root Cause**: The type checker doesn't handle `Type::EnumValueRef` for property access, falling through to the default case that expects a class.

#### 2. Misleading v0.206.0 Migration Warnings

**Location**: `engine/baml-lib/jinja/src/evaluate_type/mod.rs:193-216`

**Problem**: The warning about string-to-enum comparisons is confusing because:
- These comparisons actually work at runtime (comparing to value names)
- The suggested syntax `MyEnum.VALUE_A` doesn't exist in BAML
- The warning appears even though the feature is intentionally supported

**Example Error**:
```
warning: Type mismatch: '(status == Active)' compares values of different types (enum Status and literal["Active"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `"VALUE_A"`).
```

#### 3. Validation vs Runtime Mismatch

The runtime implementation (`MinijinjaBamlEnumValue` in `baml_value_to_jinja_value.rs`) correctly supports:
- Enum-to-string comparisons (using value name, not alias)
- Property access (`.value`, `.alias`, `.display`)
- Ordering comparisons with strings

But the validator generates incorrect warnings for these valid operations.

### Proposed Fixes

#### Fix 1: Add EnumValueRef Property Handling
```rust
// In expr.rs, add before the Type::Unknown case
Type::EnumValueRef(e) => {
    let (t, err) = types.check_enum_value_property(
        &pretty_print(&expr.expr),
        e,
        expr.name,
        expr.span(),
    );
    if let Some(e) = err {
        state.errors.push(e);
    }
    t
}
```

#### Fix 2: Update or Remove Migration Warning
Either:
- Remove the warning entirely since the comparison is intentionally supported
- Update to clarify behavior: "Enum-to-string comparisons use the enum's value name, not its alias"
- Change from warning to info level

#### Fix 3: Add Helper Method for Enum Value Properties
```rust
// In types.rs
pub fn check_enum_value_property(
    &self,
    expr: &str,
    enum_ref: &EnumValueRef,
    property: &str,
    span: Span,
) -> (Type, Option<TypeError>) {
    match property {
        "value" => (Type::String, None),
        "alias" => (Type::Union(vec![Type::String, Type::Undefined]), None),
        "display" => (Type::String, None),
        _ => (Type::Unknown, Some(TypeError::new(
            format!("Enum '{}' has no property '{}'", enum_ref.name, property),
            span,
        )))
    }
}
```

### Files Requiring Updates

1. **`engine/baml-lib/jinja/src/evaluate_type/expr.rs`** - Add EnumValueRef case for property access
2. **`engine/baml-lib/jinja/src/evaluate_type/mod.rs`** - Improve enum comparison error messages
3. **`engine/baml-lib/jinja/src/evaluate_type/types.rs`** - Add enum value property validation
4. **Test files** - Update expected warnings after fixes

### Impact Analysis

These changes will:
- Eliminate false warnings for valid enum operations
- Improve developer experience with clearer error messages
- Align validator behavior with runtime implementation
- Maintain type safety while reducing confusion

## Conclusion

The enum-to-string comparison limitation is a result of both Mini Jinja's API design and BAML's type safety philosophy. While direct comparison isn't possible without modifying Mini Jinja, the proposed filter and property approaches provide clean, maintainable solutions that preserve type safety while giving users the flexibility they need.

The recommended approach (combining custom filter and properties) can be implemented quickly without external dependencies or maintenance burden, making it an ideal solution for the current needs.

Additionally, the validation system needs updates to properly handle enum property access and provide clearer error messages that align with the actual runtime behavior.

## Enhanced Error Messages with Enum Suggestions (Added 2025-08-18)

### Requirements

1. **Suggest proper enum syntax** when users compare enums to strings (e.g., suggest `Category.Refund` when user writes `enum_arg == "Refund"`)
2. **Maintain backwards compatibility** - enum-to-string comparisons must continue working at runtime
3. **Use similarity matching** to provide helpful suggestions even with typos

### Current Infrastructure

#### Existing Suggestion System

**Location**: `engine/baml-lib/baml-core/src/ir/ir_helpers/error_utils.rs`

```rust
pub fn sort_by_match<'a, I, T>(
    name: &str,
    options: &'a I,
    max_return: Option<usize>
) -> Vec<&'a str>
```
- Uses `strsim::osa_distance` for similarity matching
- Maximum distance threshold: 20 characters
- Already used throughout codebase for "Did you mean" suggestions

#### Enum Access During Validation

**Location**: `engine/baml-lib/parser-database/src/walkers/enum.rs`

- `EnumWalker::values()` - iterate over enum values
- `EnumWalker::find_value(name)` - find specific value
- `EnumValueWalker::name()` - get value name
- Database access via `self.db.walk_enums()`

### Implementation Challenge

The current `Type::EnumValueRef(String)` only stores the value name, not the enum type. This makes it impossible to provide proper suggestions without knowing which enum type a variable belongs to.

### Proposed Solution

#### Option A: Enhanced Type Information (Recommended)

**Step 1**: Modify the type system to track enum type information:

```rust
// In engine/baml-lib/jinja/src/evaluate_type/types.rs
pub enum Type {
    // Change from:
    // EnumValueRef(String),
    // To:
    EnumValueRef { 
        enum_type: String,  // e.g., "Category"
        value: String,      // e.g., "Refund"
    },
    // ...
}
```

**Step 2**: Update type construction in walkers:

```rust
// In engine/baml-lib/parser-database/src/walkers/mod.rs:327
// Change from:
Type::EnumValueRef(idn.to_string())
// To:
Type::EnumValueRef {
    enum_type: enum_walker.name().to_string(),
    value: idn.to_string(),
}
```

**Step 3**: Enhanced error generation with suggestions:

```rust
// In engine/baml-lib/jinja/src/evaluate_type/mod.rs
impl TypeError {
    fn new_invalid_enum_cmp_with_suggestions(
        expr: &Expr,
        lhs: &Type,
        rhs: &Type,
        span: Span,
        db: &ParserDatabase,  // Add database access
    ) -> Self {
        match (lhs, rhs) {
            (Type::EnumValueRef { enum_type, .. }, Type::Literal(LiteralValue::String(s))) |
            (Type::Literal(LiteralValue::String(s)), Type::EnumValueRef { enum_type, .. }) => {
                // Get enum values from database
                let enum_walker = db.find_enum(enum_type);
                let enum_values: Vec<String> = enum_walker
                    .values()
                    .map(|v| v.name().to_string())
                    .collect();
                
                // Find similar values
                let suggestions = sort_by_match(s, &enum_values, Some(3));
                
                // Format suggestions with proper syntax
                let suggestion_text = if !suggestions.is_empty() {
                    let formatted: Vec<String> = suggestions
                        .iter()
                        .map(|v| format!("{}.{}", enum_type, v))
                        .collect();
                    format!(
                        "\n\nDid you mean {}?",
                        if formatted.len() == 1 {
                            format!("`{}`", formatted[0])
                        } else {
                            format!("one of these: `{}`", formatted.join("`, `"))
                        }
                    )
                } else {
                    format!(
                        "\n\nValid values for {} are: {}",
                        enum_type,
                        enum_values.join(", ")
                    )
                };
                
                Self {
                    message: format!(
                        "Comparing enum {} to string \"{}\". For type safety, use the explicit enum syntax: {}.VALUE{}",
                        enum_type,
                        s,
                        enum_type,
                        suggestion_text
                    ),
                    span,
                }
            }
            // ... other cases
        }
    }
}
```

#### Option B: Context-Based Approach (Less Invasive)

If modifying the type system is too complex, pass context through the validation pipeline:

```rust
// Add to validation state
struct ValidationState {
    // ... existing fields
    enum_context: HashMap<String, String>,  // var_name -> enum_type
}

// Track enum types when processing function parameters
// Use context when generating errors
```

### Implementation Steps

1. **Phase 1: Type System Enhancement**
   - Modify `Type::EnumValueRef` to include enum type
   - Update all type construction sites
   - Run tests to ensure no breakage

2. **Phase 2: Suggestion Logic**
   - Import `sort_by_match` utility into jinja validation
   - Enhance `new_invalid_enum_cmp` with suggestions
   - Add database access to error generation

3. **Phase 3: Message Formatting**
   - Change from "warning" to "info" level for backwards-compatible comparisons
   - Format suggestions clearly with proper enum syntax
   - Include all valid values if no close matches

4. **Phase 4: Testing**
   - Update test expectations in validation files
   - Add test cases for typo suggestions
   - Verify backwards compatibility maintained

### Example Error Messages

#### Before:
```
warning: Type mismatch: '(category == "Refund")' compares values of different types (enum Category and literal["Refund"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `"VALUE_A"`).
```

#### After (exact match):
```
info: Comparing enum Category to string "Refund". For type safety, use the explicit enum syntax: Category.Refund

Did you mean `Category.Refund`?
```

#### After (typo):
```
info: Comparing enum Category to string "Refnd". For type safety, use the explicit enum syntax: Category.VALUE

Did you mean one of these: `Category.Refund`, `Category.Payment`?
```

#### After (no match):
```
info: Comparing enum Category to string "Unknown". For type safety, use the explicit enum syntax: Category.VALUE

Valid values for Category are: Refund, Payment, Adjustment
```

### Benefits

1. **Better Developer Experience**: Clear, actionable suggestions
2. **Type Safety Guidance**: Promotes best practices without breaking changes
3. **Typo Tolerance**: Helps catch common mistakes
4. **Educational**: Teaches proper enum syntax through examples

### Backwards Compatibility

- Runtime behavior remains unchanged
- Enum-to-string comparisons continue to work
- Only error messages are improved
- Consider changing from "warning" to "info" level since it's not actually an error

### Dependencies

- `strsim` crate (already in dependencies)
- Access to `ParserDatabase` during validation
- Existing error formatting infrastructure