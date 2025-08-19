# Runtime Implementation Plan: Enum-String Comparison Support

## Overview

This document outlines the implementation plan for enabling runtime enum-string comparisons in BAML Jinja templates. The type checking improvements have been completed (see `implemented-type-checker-plan.md`), and now we need to enable the actual runtime behavior.

## Current Status

### ✅ Already Implemented

1. **`MinijinjaBamlEnumValue`** - Runtime enum representation
   - Located: `/Users/sam/baml2/engine/baml-lib/jinja-runtime/src/baml_value_to_jinja_value.rs:172`
   - Contains `value` (enum name) and optional `alias`
   - Has `value_cmp` method that compares against strings using value name

2. **Comprehensive Runtime Tests** - Full test coverage for enum comparisons
   - Located: `/Users/sam/baml2/engine/baml-lib/jinja-runtime/src/test_enum_comparison.rs`
   - Tests value name vs alias comparisons
   - Tests ordering consistency  
   - Tests commutativity
   - Tests edge cases (case sensitivity, non-string types)

### ❌ Currently Blocked

**Problem**: The bidirectional comparison logic in MiniJinja is commented out, preventing enum-string comparisons from working at runtime.

**Location**: `/Users/sam/baml2/third_party/minijinja/minijinja/src/value/mod.rs:513-526`

## Required Changes

### 1. Uncomment Bidirectional Check in MiniJinja

**File**: `/Users/sam/baml2/third_party/minijinja/minijinja/src/value/mod.rs`

**Lines 513-526** need to be uncommented:

```rust
// Currently commented:
// TODO: sam was here
// Try value_cmp from left side for cross-type comparisons
// if let Some(a) = self.as_object() {
//     if let Some(rv) = a.value_cmp(other) {
//         return rv == Ordering::Equal;
//     }
// }

// // Try value_cmp from right side for commutativity
// if let Some(b) = other.as_object() {
//     if let Some(rv) = b.value_cmp(self) {
//         return rv == Ordering::Equal;
//     }
// }
```

**Should become**:

```rust
// Try value_cmp from left side for cross-type comparisons
if let Some(a) = self.as_object() {
    if let Some(rv) = a.value_cmp(other) {
        return rv == Ordering::Equal;
    }
}

// Try value_cmp from right side for commutativity
if let Some(b) = other.as_object() {
    if let Some(rv) = b.value_cmp(self) {
        return rv == Ordering::Equal;
    }
}
```

### 2. Verify Implementation Details

The `MinijinjaBamlEnumValue::value_cmp` implementation is already correct:

```rust
fn value_cmp(self: &Arc<Self>, other: &minijinja::Value) -> Option<std::cmp::Ordering> {
    // Compare to strings - compare against value name only, NOT alias
    if let Some(other_str) = other.as_str() {
        return Some(self.value.as_str().cmp(other_str));
    }
    
    // Delegate to custom_cmp for object comparisons
    if let Some(other_obj) = other.as_object() {
        return self.custom_cmp(other_obj);
    }
    
    None // Cannot compare with other types
}
```

## Testing Strategy

### 1. Runtime Test Verification

**Current failing tests**:
- `internal-baml-jinja test_enum_comparison::tests::test_enum_string_comparison_value_name`
- `internal-baml-jinja test_enum_comparison::tests::test_enum_string_comparison_no_alias`

These tests verify:
- `enum_val == "ValueName"` returns `true`
- `"ValueName" == enum_val` returns `true` (commutativity)
- `enum_val == "alias"` returns `false` (aliases not used for comparison)

**After uncommenting the changes, run**:
```bash
cd /Users/sam/baml2/engine
cargo test -p internal-baml-jinja test_enum_comparison
```

### 2. Template Integration Tests

**File**: `/Users/sam/baml2/engine/baml-lib/jinja-runtime/src/test_enum_template.rs`

Tests actual template rendering with enum comparisons:
```jinja
{%- if status == "InProgress" -%}
    Status matches value name
{%- endif -%}
```

**Run with**:
```bash
cargo test -p baml-jinja-runtime test_enum_template
```

### 3. Validation File Tests

**Files**:
- `/Users/sam/baml2/engine/baml-lib/baml/tests/validation_files/enum/enum_string_comparison.baml`
- `/Users/sam/baml2/engine/baml-lib/baml/tests/validation_files/enum/enum_comparison_edge_cases.baml`

These files contain comprehensive test scenarios that should work after runtime implementation.

**Run with**:
```bash
UPDATE_EXPECT=1 cargo nextest run enum_comparison_edge_cases enum_string_comparison
```

## Implementation Behavioral Specification

### Enum-String Comparison Rules

1. **Value Name Comparison**: `enum == "ValueName"` ✅
   - Compares against the enum's declared value name
   - Case-sensitive matching
   - Works bidirectionally: `"ValueName" == enum`

2. **Alias Ignored in Comparison**: `enum == "alias_value"` ❌
   - Aliases are used for display only, not comparison
   - Should return `false` even if alias matches

3. **Display vs Comparison Separation**:
   - **Display**: `{{ enum }}` → shows alias if available, otherwise value name
   - **Comparison**: `{% if enum == "string" %}` → always uses value name

4. **Ordering Support**: `enum < "ZZZ"`
   - Alphabetical comparison using value name
   - Consistent with equality semantics

### Cross-Type Comparison Rules

1. **Enum to Enum**: Already working via `custom_cmp`
2. **Enum to String**: Enable via `value_cmp` (this plan)
3. **Enum to Other Types**: Return `false`/`None` (already working)

## Testing Checklist

After implementation, verify these behaviors:

### ✅ Basic Equality
- [ ] `enum_val == "ValueName"` → `true`
- [ ] `"ValueName" == enum_val` → `true`
- [ ] `enum_val == "alias"` → `false`
- [ ] `"alias" == enum_val` → `false`

### ✅ Ordering
- [ ] `enum_val < "ZZZ"` → correct ordering
- [ ] `"AAA" < enum_val` → correct ordering
- [ ] Ordering consistency: `(a < b) == !(b <= a)`

### ✅ Template Integration
- [ ] `{% if enum == "Value" %}` works in templates
- [ ] `{{ enum }}` displays alias when available
- [ ] Complex conditions: `{% if enum == "A" or enum == "B" %}`

### ✅ Type Safety
- [ ] `enum == 123` → `false`
- [ ] `enum == null` → `false`
- [ ] Cross-enum comparison still generates type errors

### ✅ Edge Cases
- [ ] Empty strings
- [ ] Unicode values
- [ ] Case sensitivity
- [ ] Nullable enums

## Performance Considerations

### Minimal Overhead
- The bidirectional check only activates when one operand is an object
- String-to-string comparisons remain fast (no change)
- Enum-to-enum comparisons remain fast (use existing `custom_cmp`)

### Expected Impact
- **Enum-string comparisons**: Enable new functionality (currently broken)
- **Other comparisons**: No performance impact
- **Memory usage**: No change

## Risk Assessment

### Low Risk
- Changes are additive to existing functionality
- Existing enum-to-enum comparisons remain unchanged
- String-to-string comparisons remain unchanged
- Type checking already prevents invalid usage

### Test Coverage
- Comprehensive test suite already exists
- Both unit tests and integration tests
- Edge cases already covered

### Rollback Plan
If issues arise, simply re-comment the bidirectional check code to restore previous behavior.

## Success Criteria

1. **All runtime tests pass**: `cargo test -p internal-baml-jinja test_enum_comparison`
2. **Template tests pass**: `cargo test -p baml-jinja-runtime test_enum_template`
3. **Validation warnings work**: Type checker produces helpful warnings
4. **Runtime behavior works**: Enum-string comparisons execute correctly
5. **Performance maintained**: No regression in non-enum comparisons

## Next Steps

1. **Uncomment the bidirectional check code** in minijinja value module
2. **Run test suite** to verify functionality
3. **Test edge cases** to ensure robustness
4. **Performance testing** if needed
5. **Update documentation** with new enum comparison behavior

This implementation will complete the enum-string comparison feature, providing both compile-time warnings (already implemented) and runtime functionality.