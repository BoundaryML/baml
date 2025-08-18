# Enum Comparison Specification

## Expected Behavior

This document outlines the exact expected behavior for enum comparisons in BAML templates.

### Example Definition

```baml
enum Category {
    Refund @alias("gimmie")
    Payment
}

template_string Foo(bar: Category) #"
    True:
    {{ bar == "Refund" }}         // ✅ Should work - comparing to enum value name
    {{ bar == Category.Refund }}  // ✅ Should work - enum to enum comparison

    False:
    {{ bar == "gimmie" }}          // ❌ Should be false - aliases are NOT used for comparison

    Print gimmie:
    {{ bar }}                      // Prints: "gimmie" (uses alias for display)
"#
```

## Core Principles

### 1. Comparison Uses Value Names, Not Aliases

**Rule**: When comparing an enum to a string, the comparison ALWAYS uses the enum's **value name**, never its alias.

```baml
enum Status {
    InProgress @alias("in_progress")
    Complete @alias("done")
}
```

| Comparison | Result | Explanation |
|------------|--------|-------------|
| `status == "InProgress"` | ✅ Can be true | Matches value name |
| `status == "Complete"` | ✅ Can be true | Matches value name |
| `status == "in_progress"` | ❌ Always false | Aliases not used for comparison |
| `status == "done"` | ❌ Always false | Aliases not used for comparison |
| `status == Status.InProgress` | ✅ Can be true | Enum-to-enum comparison |

### 2. Display Uses Aliases

**Rule**: When an enum is converted to a string for display (via `{{ enum_var }}`), it uses the alias if available, otherwise the value name.

```jinja
{% set status = Status.InProgress %}
{{ status }}                    // Outputs: "in_progress" (alias)

{% set category = Category.Payment %}
{{ category }}                  // Outputs: "Payment" (no alias defined)
```

### 3. Explicit Property Access

**Rule**: Enums should provide explicit properties to access both the value name and alias:

```jinja
{{ status.value }}             // Always returns: "InProgress" (value name)
{{ status.alias }}             // Returns: "in_progress" (alias) or null if no alias
{{ status.display }}           // Returns: "in_progress" (alias if exists, else value name)
```

## Comparison Matrix

Given this enum:
```baml
enum Priority {
    High @alias("urgent")
    Medium
    Low @alias("whenever")
}
```

And variable: `{% set p = Priority.High %}`

| Expression | Result | Type | Notes |
|------------|--------|------|-------|
| `p == "High"` | `true` | String comparison | Compares to value name |
| `p == "urgent"` | `false` | String comparison | Aliases not used in comparison |
| `p == Priority.High` | `true` | Enum comparison | Same enum value |
| `p.value == "High"` | `true` | String comparison | Explicit value access |
| `p.alias == "urgent"` | `true` | String comparison | Explicit alias access |
| `{{ p }}` | `"urgent"` | String output | Uses alias for display |
| `p\|string == "urgent"` | `true` | String comparison | Filter converts using alias |

## Implementation Requirements

### For String-to-Enum Comparison

```rust
impl Object for MinijinjaBamlEnumValue {
    fn value_cmp(self: &Arc<Self>, other: &Value) -> Option<Ordering> {
        if let Some(other_str) = other.as_str() {
            // Compare ONLY against the value name, NOT the alias
            Some(self.value.cmp(other_str))
        } else if let Some(other_obj) = other.as_object() {
            // Handle enum-to-enum comparison
            if let Some(other_enum) = other_obj.downcast_ref::<Self>() {
                Some(self.value.cmp(&other_enum.value))
            } else {
                None
            }
        } else {
            None
        }
    }
}
```

### For Display/Stringify

```rust
impl fmt::Display for MinijinjaBamlEnumValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Use alias if available, otherwise use value name
        write!(f, "{}", self.alias.as_ref().unwrap_or(&self.value))
    }
}
```

### For Property Access

```rust
impl Object for MinijinjaBamlEnumValue {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "value" => Some(Value::from(self.value.clone())),
            "alias" => self.alias.as_ref().map(|a| Value::from(a.clone())),
            "display" => Some(Value::from(
                self.alias.as_ref().unwrap_or(&self.value).clone()
            )),
            _ => None,
        }
    }
}
```

## Rationale

### Why Not Compare Against Aliases?

1. **Predictability**: Aliases are for display/serialization, not identity
2. **Consistency**: The enum's identity is its value name, not its presentation
3. **Refactoring Safety**: Changing an alias shouldn't break comparisons
4. **Type Clarity**: Makes it clear when comparing identity vs. presentation

### Why Allow String Comparison At All?

1. **Convenience**: Common use case in templates
2. **Migration**: Easier to migrate from string-based to enum-based APIs
3. **Interoperability**: Templates often deal with string data from external sources

## Edge Cases

### 1. No Alias Defined
```baml
enum Simple {
    ValueA  // No alias
}
```
- `simple == "ValueA"` → `true`
- `{{ simple }}` → `"ValueA"`
- `simple.alias` → `null`

### 2. Alias Same as Value
```baml
enum Redundant {
    Active @alias("Active")  // Alias same as value
}
```
- `redundant == "Active"` → `true`
- `{{ redundant }}` → `"Active"`
- Behaves identically to having no alias

### 3. Case Sensitivity
```baml
enum CaseSensitive {
    MyValue @alias("my-value")
}
```
- `cs == "MyValue"` → `true`
- `cs == "myvalue"` → `false` (case sensitive)
- `cs == "my-value"` → `false` (alias not used)

## Migration Guide

### From Strings to Enums

Before (using strings):
```jinja
{% if status == "in_progress" %}
```

After (using enums):
```jinja
{# Option 1: Compare to value name #}
{% if status == "InProgress" %}

{# Option 2: Compare to enum value #}
{% if status == Status.InProgress %}

{# Option 3: Use explicit property #}
{% if status.display == "in_progress" %}
```

### Maintaining Backward Compatibility

If you need to maintain compatibility with existing string-based templates:

```jinja
{# Create a filter that converts enum to its display string #}
{% if status|display == "in_progress" %}
```

## Testing Requirements

Tests should verify:

1. ✅ Enum-to-string comparison uses value name only
2. ✅ Enum-to-enum comparison works
3. ✅ String-to-enum comparison is commutative
4. ✅ Alias is NOT used for comparison
5. ✅ Display/stringify uses alias when available
6. ✅ Properties (.value, .alias, .display) work correctly
7. ✅ Comparison is case-sensitive
8. ✅ Null/undefined aliases handled gracefully

## Conclusion

This specification provides a clear, consistent model for enum comparisons that:
- Maintains type safety
- Provides convenience for common use cases
- Clearly separates identity (value name) from presentation (alias)
- Avoids confusion about when values match