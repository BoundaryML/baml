// Test file for nullable enum type checking in Jinja expressions
// This validates that nullable enums (Status?) generate proper warnings
// while union types with strings (Status | string) do not.

enum Status {
    Active
    Inactive @alias("inactive") 
    Pending
}

enum Priority {
    High
    Medium @alias("med")
    Low
}

class MyInput {
    // Nullable enums - should trigger warnings when compared to strings
    nullable_status Status?
    nullable_priority Priority?
    
    // Union with string - should NOT trigger enum warnings
    status_or_string (Status | string)?
    enum_with_string (Status | string | null)?
    complex_union (Status | Priority | string)?
    
    // Other types for edge cases
    nullable_string string?
    int_or_enum (int | Status)?
    regular_status Status
    regular_string string
}

function TestNullableEnumComparisons(input: MyInput) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
    Test cases for nullable enum comparisons that SHOULD generate warnings:
    
    // Basic nullable enum to string literal comparisons
    {% if input.nullable_status == "Active" %}
        Status is Active
    {% elif input.nullable_status == "inactive" %}
        Status is inactive (alias)
    {% elif "Pending" == input.nullable_status %}
        Status is Pending (reversed)
    {% elif input.nullable_status != "Inactive" %}
        Status is not Inactive
    {% endif %}
    
    // Ordering comparisons with nullable enums
    {% if input.nullable_status > "AAA" %}
        Status comes after AAA alphabetically
    {% elif input.nullable_status <= "Zzz" %}
        Status comes before or equals Zzz
    {% elif input.nullable_status >= "Active" %}
        Status is Active or comes after
    {% endif %}
    
    // Multiple nullable enum comparisons
    {% if input.nullable_priority == "High" %}
        Priority is High
    {% elif input.nullable_priority != "Low" %}
        Priority is not Low
    {% endif %}
    
    // Complex boolean expressions with nullable enums
    {% if (input.nullable_status == "Active") and (input.nullable_priority != "Low") %}
        Active status and not low priority
    {% endif %}
    
    // Safe navigation patterns (still should warn on the enum comparison)
    {% if input.nullable_status and input.nullable_status == "Pending" %}
        Safely checked status is Pending
    {% endif %}
    "#
}

function TestCrossNullableEnumComparisons(input: MyInput, other_input: MyInput) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
    Test cases for cross-nullable-enum comparisons:
    
    // Same enum type nullable comparisons (should be OK)
    {% if input.nullable_status == other_input.nullable_status %}
        Both have same status
    {% endif %}
    
    // Different enum type comparisons (should error)
    {% if input.nullable_status == input.nullable_priority %}
        Comparing Status? to Priority? (should error)
    {% elif input.nullable_status != input.nullable_priority %}
        Different enum types comparison (should error)
    {% endif %}
    "#
}

function TestNullComparisons(input: MyInput) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
    Test cases for null comparisons with nullable enums:
    
    // Null comparisons (should not trigger enum warnings)
    {% if none == input.nullable_status %}
        Status is null
    {% elif input.nullable_status == none %}
        Status is null (reversed)
    {% endif %}
    
    // Fallback patterns
    {% if input.nullable_status or "Default" %}
        Status or fallback
    {% endif %}
    "#
}

function TestUnionTypesNoWarnings(input: MyInput) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
    Test cases that should NOT generate enum warnings:
    
    // Status | string comparisons (should NOT warn)
    {% if input.status_or_string == "Active" %}
        Could be enum or string - no warning
    {% elif input.status_or_string == "SomeString" %}
        Definitely a string value - no warning
    {% endif %}
    
    // Complex union types (should NOT warn)
    {% if input.enum_with_string == "Active" %}
        Status | string | null - no warning
    {% elif input.complex_union == "Test" %}
        Status | Priority | string - no warning  
    {% endif %}
    
    // Non-enum types (should NOT warn)
    {% if input.nullable_string == "Value" %}
        string? comparison - no warning
    {% elif input.int_or_enum == "123" %}
        int | Status comparison - no warning
    {% endif %}
    
    // Regular enum and string (existing behavior)
    {% if input.regular_status == "Active" %}
        Regular enum comparison (should warn as before)
    {% elif input.regular_string == "Value" %}
        String comparison (no warning)
    {% endif %}
    "#
}

// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:40
//    | 
// 39 |     // Basic nullable enum to string literal comparisons
// 40 |     {% if input.nullable_status == "Active" %}
//    | 
// warning: Use `Status.Inactive` instead of "inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:42
//    | 
// 41 |         Status is Active
// 42 |     {% elif input.nullable_status == "inactive" %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:44
//    | 
// 43 |         Status is inactive (alias)
// 44 |     {% elif "Pending" == input.nullable_status %}
//    | 
// warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:46
//    | 
// 45 |         Status is Pending (reversed)
// 46 |     {% elif input.nullable_status != "Inactive" %}
//    | 
// warning: Use one of: Status.Active, Status.Inactive - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:51
//    | 
// 50 |     // Ordering comparisons with nullable enums
// 51 |     {% if input.nullable_status > "AAA" %}
//    | 
// warning: Use one of: Status.Active, Status.Pending, Status.Inactive - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:53
//    | 
// 52 |         Status comes after AAA alphabetically
// 53 |     {% elif input.nullable_status <= "Zzz" %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:55
//    | 
// 54 |         Status comes before or equals Zzz
// 55 |     {% elif input.nullable_status >= "Active" %}
//    | 
// warning: Use `Priority.High` instead of "High" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:60
//    | 
// 59 |     // Multiple nullable enum comparisons
// 60 |     {% if input.nullable_priority == "High" %}
//    | 
// warning: Use `Priority.Low` instead of "Low" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:62
//    | 
// 61 |         Priority is High
// 62 |     {% elif input.nullable_priority != "Low" %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:67
//    | 
// 66 |     // Complex boolean expressions with nullable enums
// 67 |     {% if (input.nullable_status == "Active") and (input.nullable_priority != "Low") %}
//    | 
// warning: Use `Priority.Low` instead of "Low" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:67
//    | 
// 66 |     // Complex boolean expressions with nullable enums
// 67 |     {% if (input.nullable_status == "Active") and (input.nullable_priority != "Low") %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:72
//    | 
// 71 |     // Safe navigation patterns (still should warn on the enum comparison)
// 72 |     {% if input.nullable_status and input.nullable_status == "Pending" %}
//    | 
// warning: Use one of: Status.Pending, Status.Inactive - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:89
//    | 
// 88 |     // Different enum type comparisons (should error)
// 89 |     {% if input.nullable_status == input.nullable_priority %}
//    | 
// warning: Use one of: Status.Pending, Status.Inactive - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:91
//    | 
// 90 |         Comparing Status? to Priority? (should error)
// 91 |     {% elif input.nullable_status != input.nullable_priority %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/nullable_enums_in_jinja.baml:143
//    | 
// 142 |     // Regular enum and string (existing behavior)
// 143 |     {% if input.regular_status == "Active" %}
//    | 
