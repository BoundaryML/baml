# Implementation Plan: Enum-String Comparison Support in BAML

## 1. Current State of the World

### What are BAML Enums?

BAML enums are a type-safe way to define a fixed set of named values, commonly used for categorization, status tracking, and controlled vocabularies in AI applications. They provide both a **value name** (used for identity/comparison) and an optional **alias** (used for display/serialization).

**Example BAML enum definition:**

```baml
enum Category {
    Refund @alias("gimmie_money_back")
    Payment @alias("take_my_money") 
    Adjustment
    Dispute @alias("fight_the_charge")
}

enum Priority {
    High @alias("urgent")
    Medium @alias("normal")
    Low @alias("whenever")
}

enum Status {
    Pending
    InProgress @alias("in_progress")
    Complete @alias("done")
    Failed @alias("error")
}

// Usage in a function
function ClassifyTransaction(
    description: string,
    amount: float
) -> Category {
    client "gpt-4o"
    prompt #"
        Classify this transaction:
        Description: {{ description }}
        Amount: ${{ amount }}
        
        Category: {{ ctx.output_format }}
    "#
}

// Usage in templates with current limitations
template_string ProcessTransaction(
    category: Category,
    priority: Priority,
    status: Status
) #"
    Transaction Details:
    Category: {{ category }}           // Outputs: "gimmie_money_back" (alias)
    Priority: {{ priority }}           // Outputs: "urgent" (alias)  
    Status: {{ status }}               // Outputs: "in_progress" (alias)
    
    {% if category == Category.Refund %}   {# ✅ This works - enum to enum #}
        Processing refund...
    {% endif %}
    
    {% if category == "Refund" %}          {# ❌ Currently broken - enum to string #}
        This should work but doesn't!
    {% endif %}
    
    {% if priority == "urgent" %}          {# ❌ Also broken - comparing to alias #}
        This is confusing for users!
    {% endif %}
"#
```

### Current Implementation Problems

BAML currently **blocks** enum-to-string comparisons in Jinja templates with explicit type checking. When users write `{% if enum_arg == "VALUE_A" %}`, they receive a non-fatal compiler warning:

```
Type mismatch: '(enum_arg == VALUE_A)' compares values of different types (enum VALUE_A and literal["VALUE_A"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values.
```

**Real-world user pain points:**

```baml
// Users naturally want to write:
{% if status == "Complete" %}
    Send completion email
{% endif %}

// But instead must write:
{% if status == Status.Complete %}
    Send completion email
{% endif %}

// Or are confused why this doesn't work:
{% if category == "gimmie_money_back" %}  {# Comparing to alias #}
    Process refund
{% endif %}
```

### Technical Details
- **Runtime Implementation**: `MinijinjaBamlEnumValue` in `engine/baml-lib/jinja-runtime/src/baml_value_to_jinja_value.rs` only supports enum-to-enum comparisons via `custom_cmp`
- **Validation System**: `engine/baml-lib/jinja/src/evaluate_type/expr.rs` explicitly blocks cross-type comparisons
- **Mini Jinja Limitation**: The `custom_cmp` method only receives `&DynObject`, not `&Value`, preventing string detection
- **Design Philosophy**: BAML deliberately chose type safety over implicit conversions in v0.206.0
- **Enum Structure**: Each enum has a `value` (the defined name) and optional `alias` (for display/serialization)

### Current Issues
1. **User Experience**: Users expect `enum == "string"` to work, because this was the behavior in baml 0.205.1 and all versions prior.
2. **Error Messages**: do not provide suggestions about what fix to apply.

## 2. Desired State of the World

### User Experience Goals
- **Backwards compatibility**: `enum_value == "ValueName"` should work naturally
- **Type Safety**: Maintain strict type checking while allowing necessary flexibility
- **Clear Semantics**: Comparisons allow using value names, display uses aliases
- **Commutativity**: Both `enum == "string"` and `"string" == enum` should work

### Behavioral Specification
```jinja
enum Category {
    Refund @alias("gimmie")
    Payment
}

// Comparison behavior (uses value names)
{{ category == "Refund" }}         // ✅ true (if category is Refund)
{{ category == "gimmie" }}          // ❌ false (aliases not used for comparison)
{{ category == Category.Refund }}  // ✅ true (enum-to-enum)

// Display behavior (uses aliases)
{{ category }}                      // "gimmie" (alias for display)
```


## 3. High-Level Design

### Implementation Strategy: Bidirectional Check Solution

Based on the analysis in `thoughts/research/commutativity-analysis.md`, we will implement **Solution 1: Bidirectional Check** to enable enum-string comparisons while maintaining commutativity and type safety.

#### Key Design Principles

1. **Commutativity**: Both `enum == "string"` and `"string" == enum` must work identically
2. **Value-based Comparison**: Comparisons use enum value names, NOT aliases
3. **Type Safety**: Invalid cross-type comparisons still produce helpful errors
4. **Performance**: Minimal overhead for existing enum-to-enum comparisons

#### Technical Architecture

```rust
// Runtime comparison logic (already implemented)
impl Object for MinijinjaBamlEnumValue {
    fn value_cmp(&self, other: &Value) -> Option<Ordering> {
        // Compare to strings using value name only
        if let Some(other_str) = other.as_str() {
            return Some(self.value.as_str().cmp(other_str));
        }
        // ... existing enum-to-enum logic
    }
}

// Bidirectional check in minijinja Value::eq (already implemented)
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        // ... primitive comparisons ...
        None => {
            // Try value_cmp from left side
            if let Some(a) = self.as_object() {
                if let Some(rv) = a.value_cmp(other) {
                    return rv == Ordering::Equal;
                }
            }
            // Try value_cmp from right side (commutativity!)
            if let Some(b) = other.as_object() {
                if let Some(rv) = b.value_cmp(self) {
                    return rv == Ordering::Equal;
                }
            }
            // ... existing object-to-object logic
        }
    }
}
```

#### Behavioral Specification

```jinja
enum Category {
    Refund @alias("gimmie_money_back")
    Payment @alias("take_my_money") 
    Adjustment
}

// ✅ These will work (value-based comparison)
{{ category == "Refund" }}         // true if category is Refund
{{ "Refund" == category }}         // true (commutative)
{{ category == Category.Refund }}  // true (enum-to-enum)

// ❌ These will NOT work (aliases not used for comparison)
{{ category == "gimmie_money_back" }}  // false - alias not used

// 📤 Display behavior (uses aliases)
{{ category }}                      // "gimmie_money_back" (alias for display)
```

### Current Implementation Status

#### ✅ Already Implemented
- **MinijinjaBamlEnumValue::value_cmp()** - Runtime string comparison logic
- **Bidirectional check in Value::eq** - Ensures commutativity in minijinja fork
- **Comprehensive test suite** - Edge cases covered in test_enum_comparison.rs

#### ❌ Needs Implementation
- **Type checker updates** - Remove blocking of enum-string comparisons
- **Improved error messages** - Suggest enum literals instead of string literals
- **Documentation completion** - Flesh out this plan document

## 4. Implementation Plan

### Phase 1: Testing Infrastructure (Week 1)
- Run baseline tests with `UPDATE_EXPECT=1 cargo nextest run enum_comparison_edge_cases enum_string_comparison`
- Verify existing tests in `test_enum_comparison.rs` cover all scenarios:
  - Basic enum-string equality and commutativity
  - Ordering consistency (`enum.cmp(string)` vs `string.cmp(enum)`)
  - Edge cases: empty strings, Unicode, case sensitivity
  - Type safety: enum vs non-string types

### Phase 2: Runtime Verification (Week 1)
- **Status**: Implementation already complete
- Verify runtime behavior matches specification:
  - `enum == "ValueName"` works (uses value, not alias)
  - `"ValueName" == enum` works (commutativity)
  - `enum != "AliasName"` correctly returns false
  - Performance impact is minimal for existing enum-enum comparisons

### Phase 3: Type Checker Updates (Week 2)
- **Location**: `engine/baml-lib/jinja/src/evaluate_type/mod.rs`
- **Goal**: Remove enum-string comparison blocking while improving error messages

#### Current Error Message (to be updated):
```rust
"Type mismatch: '{}' compares values of different types ({} and {}). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `\"VALUE_A\"`)."
```

#### Proposed Improvements:
1. **Allow enum-string comparisons** - Remove the type error for valid enum-string pairs
2. **Suggest enum literals** - When user compares to string literal, suggest enum value
3. **Context-aware suggestions** - Use actual enum name and values in suggestions

```rust
// New error for string literal suggestions
"Consider using enum value: Instead of '{}' == \"{}\", use '{}' == {}.{} for better type safety"
```

#### Implementation Strategy:
- Modify `new_invalid_enum_cmp` to distinguish between:
  - Valid enum-string comparisons (allow)
  - String literals that could be enum values (suggest enum literal)
  - Invalid cross-type comparisons (block with helpful message)

### Phase 4: Documentation & Validation (Week 2)
- Complete `thoughts/plan.md` with final implementation details
- Update user-facing documentation to explain new behavior
- Run comprehensive test suite to ensure no regressions
- Performance benchmarking to verify minimal overhead

## 5. Risk Mitigation

### Potential Issues & Solutions

1. **Performance Regression**
   - **Risk**: Double virtual calls for failed object comparisons
   - **Mitigation**: Short-circuit evaluation, only affects object-string comparisons
   - **Measurement**: Benchmark existing enum-heavy templates

2. **Type Safety Concerns**
   - **Risk**: Users might accidentally compare incompatible types
   - **Mitigation**: Maintain strict type checking for non-enum objects
   - **Testing**: Comprehensive edge case coverage

3. **Migration Complexity**
   - **Risk**: Existing templates might behave differently
   - **Mitigation**: This change is additive - makes previously failing comparisons work
   - **Validation**: Test against existing BAML templates

4. **Debugging Complexity**
   - **Risk**: Non-obvious comparison paths in templates
   - **Mitigation**: Clear error messages and documentation
   - **Tooling**: Consider debug output showing which comparison path was taken

## 6. Success Metrics

### Functional Requirements
- ✅ `enum_value == "ValueName"` returns true when values match
- ✅ `"ValueName" == enum_value` returns true (commutativity)
- ✅ Comparisons use value names, not aliases
- ✅ Invalid comparisons produce helpful error messages
- ✅ Performance impact < 5% for enum-heavy templates

### User Experience Goals
- ✅ Natural, intuitive syntax for enum comparisons
- ✅ Clear error messages with actionable suggestions
- ✅ Backwards compatibility with existing enum-enum comparisons
- ✅ Consistent behavior across all template contexts

### Technical Validation
- ✅ All existing tests pass without modification
- ✅ New comprehensive test suite covers edge cases
- ✅ Type checker provides helpful guidance
- ✅ Runtime behavior matches specification exactly

