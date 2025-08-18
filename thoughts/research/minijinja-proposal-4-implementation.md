# Proposal 4: Implementing Cross-Type Comparisons in Mini Jinja

## Problem Statement

Currently, Mini Jinja's `custom_cmp` method in the `Object` trait only receives a `&DynObject` parameter, which prevents custom objects from comparing against primitive types like strings. This limitation makes it impossible to implement comparisons like `enum_value == "STRING_VALUE"` in BAML templates.

## Current Architecture

### How Comparisons Work Now

1. **Value Comparison Flow** (`minijinja/src/value/mod.rs:496-572`):
   ```rust
   impl PartialEq for Value {
       fn eq(&self, other: &Self) -> bool {
           // ... primitive type comparisons ...
           if let (Some(a), Some(b)) = (self.as_object(), other.as_object()) {
               if a.is_same_object_type(b) {
                   if let Some(rv) = a.custom_cmp(b) {
                       return rv == Ordering::Equal;
                   }
               }
           }
       }
   }
   ```

2. **Object Trait Definition** (`minijinja/src/value/object.rs:272-275`):
   ```rust
   fn custom_cmp(self: &Arc<Self>, other: &DynObject) -> Option<Ordering> {
       let _ = other;
       None
   }
   ```

3. **DynObject Creation** (`minijinja/src/value/object.rs:type_erase! macro`):
   - The `DynObject` is a type-erased wrapper created by the `type_erase!` macro
   - It only holds a pointer to the underlying object and its vtable
   - No access to the original `Value` that contains it

### The Core Issue

When comparing `Value::Object(DynObject)` with `Value::String`, the comparison fails at line 509 because:
1. `other.as_object()` returns `None` for strings
2. The comparison falls through to line 566 returning `false`
3. Even if we tried to call `custom_cmp`, we can't pass the string `Value` to it

## Proposed Solutions

### Solution A: Add a New Comparison Method (RECOMMENDED)

Add a new method to the `Object` trait that receives the full `Value`:

```rust
// In minijinja/src/value/object.rs

pub trait Object: Debug + Send + Sync {
    // ... existing methods ...
    
    /// Custom comparison that receives the full Value for cross-type comparisons.
    /// This is called before custom_cmp and allows comparing with non-object types.
    fn value_cmp(self: &Arc<Self>, other: &Value) -> Option<Ordering> {
        // Default implementation delegates to custom_cmp for objects
        if let Some(other_obj) = other.as_object() {
            self.custom_cmp(other_obj)
        } else {
            None
        }
    }
    
    // Keep existing custom_cmp for backward compatibility
    fn custom_cmp(self: &Arc<Self>, other: &DynObject) -> Option<Ordering> {
        let _ = other;
        None
    }
}
```

**Changes to Value comparison** (`minijinja/src/value/mod.rs`):
```rust
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        // ... existing code ...
        None => {
            if let Some(a) = self.as_object() {
                // Try value_cmp first for cross-type comparisons
                if let Some(rv) = a.value_cmp(other) {
                    return rv == Ordering::Equal;
                }
            }
            // ... existing object comparison code ...
        }
    }
}
```

**Update type_erase! macro** to include `value_cmp` in the vtable.

### Solution B: Modify custom_cmp Signature (BREAKING CHANGE)

Change `custom_cmp` to receive `&Value` instead of `&DynObject`:

```rust
fn custom_cmp(self: &Arc<Self>, other: &Value) -> Option<Ordering>
```

**Pros:**
- Simpler API surface
- More flexible for all use cases

**Cons:**
- **BREAKING CHANGE** for all existing implementations
- Requires updating all code using custom_cmp

### Solution C: Add Comparison Context (COMPLEX)

Create a comparison context that holds both the DynObject and original Value:

```rust
pub struct ComparisonContext<'a> {
    pub object: Option<&'a DynObject>,
    pub value: &'a Value,
}

fn custom_cmp(self: &Arc<Self>, other: ComparisonContext) -> Option<Ordering>
```

**Cons:**
- More complex API
- Still a breaking change

## Implementation Plan for Solution A

### Phase 1: Core Changes
1. Add `value_cmp` method to `Object` trait with default implementation
2. Update `type_erase!` macro to include `value_cmp` in vtable
3. Modify `Value::eq` and `Value::cmp` to call `value_cmp` first

### Phase 2: BAML Integration
1. Implement `value_cmp` for `MinijinjaBamlEnumValue`:
   ```rust
   fn value_cmp(self: &Arc<Self>, other: &Value) -> Option<Ordering> {
       // Try object comparison first (existing enum-to-enum)
       if let Some(other_obj) = other.as_object() {
           if let Some(other_enum) = other_obj.downcast_ref::<Self>() {
               return Some(self.value.cmp(&other_enum.value)
                   .then(self.alias.cmp(&other_enum.alias)));
           }
       }
       
       // Try string comparison
       if let Some(other_str) = other.as_str() {
           // Compare against value or alias
           if self.value == other_str || 
              self.alias.as_deref() == Some(other_str) {
               return Some(Ordering::Equal);
           } else {
               return Some(self.value.cmp(other_str));
           }
       }
       
       None
   }
   ```

### Phase 3: Testing
1. Add Mini Jinja tests for cross-type comparisons
2. Update BAML tests to verify enum-string comparisons work
3. Ensure backward compatibility with existing custom_cmp implementations

## Impact Analysis

### Backward Compatibility
- **Solution A**: ✅ Fully backward compatible (new method with default implementation)
- **Solution B**: ❌ Breaking change for all custom_cmp users
- **Solution C**: ❌ Breaking change for all custom_cmp users

### Performance Impact
- Minimal - one additional virtual call in comparison path
- Only affects objects, not primitive types
- Default implementation fast-paths to existing behavior

### API Complexity
- **Solution A**: Small increase (one new optional method)
- **Solution B**: No change in complexity
- **Solution C**: Moderate increase (new type needed)

## Files to Modify

### Mini Jinja Core:
1. `minijinja/src/value/object.rs` - Add `value_cmp` method
2. `minijinja/src/value/mod.rs` - Update comparison logic
3. `minijinja/src/value/type_erase.rs` - Update macro if needed
4. `minijinja/tests/test_value.rs` - Add tests

### BAML:
1. `engine/baml-lib/jinja-runtime/src/baml_value_to_jinja_value.rs` - Implement `value_cmp`
2. `engine/baml-lib/jinja/src/evaluate_type/test_expr.rs` - Update tests

## Example Usage After Implementation

```rust
// In BAML's MinijinjaBamlEnumValue
impl Object for MinijinjaBamlEnumValue {
    fn value_cmp(self: &Arc<Self>, other: &Value) -> Option<Ordering> {
        // Handle both enum-to-enum and enum-to-string comparisons
        match other.as_str() {
            Some(s) => Some(self.value.cmp(s)),
            None => {
                // Fall back to object comparison
                if let Some(obj) = other.as_object() {
                    self.custom_cmp(obj)
                } else {
                    None
                }
            }
        }
    }
}
```

Template usage:
```jinja
{% if enum_arg == "VALUE_A" %}  {# Now works! #}
    Enum matches string
{% endif %}

{% if enum_arg == MyEnum.VALUE_A %}  {# Still works #}
    Enum matches enum
{% endif %}
```

## Conclusion

**Recommendation**: Implement Solution A (`value_cmp` method) because:
1. It's fully backward compatible
2. Provides the flexibility needed for cross-type comparisons
3. Has minimal performance impact
4. Follows Mini Jinja's pattern of providing sensible defaults

This solution elegantly solves the enum-to-string comparison problem while maintaining Mini Jinja's clean API and backward compatibility.