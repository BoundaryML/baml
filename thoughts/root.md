# Enum-String Comparison Research

## Overview
This research explores how to enable enum-to-string comparisons in Mini Jinja templates within the BAML codebase. The core issue is that comparisons like `{% if enum_arg == "VALUE_A" %}` currently fail with a type mismatch error.

## Research Documents

### 1. [Enum Comparison Specification](research/enum-comparison-specification.md)
**Summary**: Clear specification of expected behavior for enum comparisons in BAML templates.

**Key Rules**:
- Comparisons use enum **value names**, NOT aliases
- `enum == "ValueName"` works, `enum == "alias"` doesn't  
- Display/stringify uses aliases when available
- Provides `.value`, `.alias`, and `.display` properties for explicit access

**Example**:
```baml
enum Category {
    Refund @alias("gimmie")
    Payment
}
// bar == "Refund" → true
// bar == "gimmie" → false  
// {{ bar }} → "gimmie"
```

---

### 2. [Initial Research: Enum-String Comparison](research/enum-string-comparison-research.md)
**Summary**: Comprehensive analysis of the current implementation and multiple proposed solutions for enabling enum-to-string comparisons in BAML's Jinja templates.

**Key Findings**:
- Current implementation explicitly blocks enum-to-string comparisons
- Mini Jinja's `custom_cmp` method only compares objects of the same type
- Four different approaches identified, ranging from custom filters to Mini Jinja modifications

**Proposed Solutions**:
1. Custom Filter Approach (easiest)
2. Add Properties to Enum Objects (most intuitive)
3. Global Helper Function
4. Enhanced custom_cmp (requires Mini Jinja changes)

---

### 3. [Proposal 4: Mini Jinja Implementation](research/minijinja-proposal-4-implementation.md)
**Summary**: Detailed implementation plan for modifying Mini Jinja to support cross-type comparisons through a new `value_cmp` method.

**Key Contributions**:
- Identified that `custom_cmp` receives `&DynObject`, not `&Value`, preventing string detection
- Proposed adding a new `value_cmp` method that receives the full `Value`
- Maintains backward compatibility while enabling cross-type comparisons
- Complete implementation plan with code examples

**Architecture Changes**:
- Add `value_cmp` to `Object` trait with default implementation
- Update `type_erase!` macro to include new method in vtable
- Modify `Value::eq` and `Value::cmp` to call `value_cmp` first

---

### 4. [Commutativity Analysis](research/commutativity-analysis.md)
**Summary**: Critical analysis of the commutativity problem in the proposed solution and how to ensure `enum == "string"` behaves the same as `"string" == enum`. **Includes important pitfalls and concerns**.

**Problem Identified**:
- Original proposal would make comparisons non-commutative
- `enum == "string"` would work but `"string" == enum` would fail

**Solution**:
- Implement bidirectional checking in `Value::eq`
- Check `value_cmp` from both directions
- For `Ord::cmp`, reverse ordering when checking from right side

**Critical Pitfalls**:
- **Transitivity violations** - If A == "foo" and B == "foo", then A should == B (but won't)
- **Hash contract violations** - Equal values must have equal hashes
- **Performance regression** - Doubles virtual calls for object/non-object comparisons
- **Type safety loss** - Implicit conversions can hide bugs
- **Migration hazards** - Existing code behavior changes silently

**Alternative Recommendation**: Use explicit approaches (filters/properties) to maintain type safety

**Code Example**:
```rust
// Check from both directions for commutativity
if let Some(a) = self.as_object() {
    if let Some(rv) = a.value_cmp(other) {
        return rv == Ordering::Equal;
    }
}
if let Some(b) = other.as_object() {
    if let Some(rv) = b.value_cmp(self) {
        return rv == Ordering::Equal;
    }
}
```

---

## Current Status

Based on the research, the recommended approach is:

### For Immediate Implementation (No Mini Jinja Fork)
Implement **Solution 1 (Custom Filter)** or **Solution 2 (Properties)** from the initial research:
- Quick to implement
- No Mini Jinja modifications needed
- Provides working solution for users

### For Long-term Solution (Mini Jinja Fork)
Implement **Proposal 4 with Bidirectional Checking**:
- Fork Mini Jinja and add `value_cmp` method
- Implement bidirectional checking for commutativity
- Submit as PR to upstream Mini Jinja

## Implementation Checklist

### BAML Changes (Can Do Now)
- [ ] Add custom filter `as_string` for enum conversion
- [ ] Add `.value` and `.name` properties to enum objects
- [ ] Update type checking to allow these operations
- [ ] Add tests for both approaches

### Mini Jinja Changes (Requires Fork)
- [ ] Add `value_cmp` method to `Object` trait
- [ ] Update `type_erase!` macro
- [ ] Implement bidirectional checking in `Value::eq`
- [ ] Implement bidirectional checking with order reversal in `Value::cmp`
- [ ] Add comprehensive tests for cross-type comparisons
- [ ] Submit PR to upstream Mini Jinja

## Key Insights

1. **Type Safety vs Flexibility**: BAML deliberately chose type safety by blocking implicit conversions, but users need flexibility for enum comparisons.

2. **API Design**: The best solution maintains backward compatibility while providing new capabilities through optional methods with sensible defaults.

3. **Commutativity Matters**: Equality operations must be commutative to match user expectations and mathematical properties.

4. **Performance Considerations**: The bidirectional check adds minimal overhead (at most 2 virtual calls) and only affects object comparisons.

## Next Steps

1. **Immediate**: Implement custom filter and properties approach in BAML
2. **Short-term**: Fork Mini Jinja and implement the `value_cmp` solution
3. **Long-term**: Work with Mini Jinja maintainers to upstream the changes

## Related Files in Codebase

### BAML Files
- `/engine/baml-lib/jinja-runtime/src/baml_value_to_jinja_value.rs` - Enum value implementation
- `/engine/baml-lib/jinja-runtime/src/lib.rs` - Runtime rendering logic
- `/engine/baml-lib/jinja/src/evaluate_type/expr.rs` - Type checking for comparisons
- `/engine/baml-lib/baml-core/src/ir/jinja_helpers.rs` - Custom filters

### Mini Jinja Files (in fork)
- `/minijinja/src/value/object.rs` - Object trait definition
- `/minijinja/src/value/mod.rs` - Value comparison implementation
- `/minijinja/src/value/type_erase.rs` - Type erasure macro
- `/minijinja/src/value/ops.rs` - Type coercion logic