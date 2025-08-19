enum Priority {
    High @alias("urgent")
    Medium
    Low @alias("whenever")
}

enum TaskStatus {
    NotStarted @alias("not_started")
    InProgress @alias("in_progress")
    Complete @alias("done")
    Cancelled
}

// Test various comparison scenarios that should generate warnings or work correctly
function TestEnumComparisonEdgeCases(priority: Priority, status: TaskStatus) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        // Test 1: Comparing enum to its value name (should work)
        {% if priority == "High" %}
            Priority is High (correct - comparing to value name)
        {% endif %}
        
        // Test 2: Comparing enum to its alias (should NOT match)
        {% if priority == "urgent" %}
            This should NOT match - comparing to alias "urgent" instead of value "High"
        {% endif %}
        
        // Test 3: Enum without alias
        {% if priority == "Medium" %}
            Priority is Medium (no alias defined)
        {% endif %}
        
        // Test 4: Case sensitivity
        {% if priority == "high" %}
            This should NOT match - wrong case
        {% endif %}
        
        // Test 5: Comparing different enum types (should be type error)
        // This should generate a type checking error
        // {% if priority == status %}
        //     This comparison should fail type checking
        // {% endif %}
        
        // Test 6: Multiple enum comparisons
        {% if status == "NotStarted" or status == "InProgress" %}
            Task is active
        {% endif %}
                
        // Test 8: Display output (should use alias if available)
        Priority: {{ priority }}
        Status: {{ status }}
        
        // Test 9: Comparison in loop
        {% for s in ["NotStarted", "InProgress", "Complete", "Cancelled"] %}
            {% if status == s %}
                Current status matches: {{ s }}
            {% endif %}
        {% endfor %}
        
        // Test 10: Complex boolean logic
        {% if (priority == "High" or priority == "Medium") and status != "Cancelled" %}
            Important task that is not cancelled
        {% endif %}
        
        // Test 11: Ordering comparisons
        {% if priority < "Medium" %}
            Priority comes before Medium alphabetically
        {% endif %}
        
        {% if status > "InProgress" %}
            Status comes after InProgress alphabetically
        {% endif %}
        
        // Test 12: Negation
        {% if not (status == "Complete") %}
            Task is not complete
        {% endif %}
    "#
}

// Test comparisons with nullable enums
function TestNullableEnumComparison(priority: Priority?, status: TaskStatus | null) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        // Check for null first
        {% if priority is none %}
            Priority is null
        {% elif priority == "High" %}
            Priority is High
        {% else %}
            Priority is {{ priority }}
        {% endif %}
        
        // Safe navigation with nullable
        {% if status and status == "Complete" %}
            Status exists and is Complete
        {% endif %}
        
        // Default value for nullable
        Priority: {{ priority if priority else "No priority set" }}
    "#
}

// warning: Consider using enum value: Instead of '(priority == High) == "High"', use '(priority == High) == Priority.High' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:19
//    | 
// 18 |         // Test 1: Comparing enum to its value name (should work)
// 19 |         {% if priority == "High" %}
//    | 
// warning: Alias detected: Instead of '(priority == urgent) == "urgent"' (alias), use '(priority == urgent) == Priority.High' (value name)
//   -->  enum/enum_comparison_edge_cases.baml:24
//    | 
// 23 |         // Test 2: Comparing enum to its alias (should NOT match)
// 24 |         {% if priority == "urgent" %}
//    | 
// warning: Consider using enum value: Instead of '(priority == Medium) == "Medium"', use '(priority == Medium) == Priority.Medium' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:29
//    | 
// 28 |         // Test 3: Enum without alias
// 29 |         {% if priority == "Medium" %}
//    | 
// warning: Did you mean 'Priority.High'? (Note: enum comparisons are case-sensitive)
//   -->  enum/enum_comparison_edge_cases.baml:34
//    | 
// 33 |         // Test 4: Case sensitivity
// 34 |         {% if priority == "high" %}
//    | 
// warning: Type mismatch: '(priority == status)' compares values of different types (enum Priority and enum TaskStatus). Enum values can only be compared with enum values.
//   -->  enum/enum_comparison_edge_cases.baml:40
//    | 
// 39 |         // This should generate a type checking error
// 40 |         // {% if priority == status %}
//    | 
// warning: Consider using enum value: Instead of '(status == NotStarted) == "NotStarted"', use '(status == NotStarted) == TaskStatus.NotStarted' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:45
//    | 
// 44 |         // Test 6: Multiple enum comparisons
// 45 |         {% if status == "NotStarted" or status == "InProgress" %}
//    | 
// warning: Consider using enum value: Instead of '(status == InProgress) == "InProgress"', use '(status == InProgress) == TaskStatus.InProgress' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:45
//    | 
// 44 |         // Test 6: Multiple enum comparisons
// 45 |         {% if status == "NotStarted" or status == "InProgress" %}
//    | 
// warning: Type mismatch: '(status == s)' compares values of different types (enum TaskStatus and (literal["Cancelled"] | literal["Complete"] | literal["InProgress"] | literal["NotStarted"])). Enum values can only be compared with enum values.
//   -->  enum/enum_comparison_edge_cases.baml:55
//    | 
// 54 |         {% for s in ["NotStarted", "InProgress", "Complete", "Cancelled"] %}
// 55 |             {% if status == s %}
//    | 
// warning: Consider using enum value: Instead of '(priority == High) == "High"', use '(priority == High) == Priority.High' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:61
//    | 
// 60 |         // Test 10: Complex boolean logic
// 61 |         {% if (priority == "High" or priority == "Medium") and status != "Cancelled" %}
//    | 
// warning: Consider using enum value: Instead of '(priority == Medium) == "Medium"', use '(priority == Medium) == Priority.Medium' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:61
//    | 
// 60 |         // Test 10: Complex boolean logic
// 61 |         {% if (priority == "High" or priority == "Medium") and status != "Cancelled" %}
//    | 
// warning: Consider using enum value: Instead of '(status != Cancelled) == "Cancelled"', use '(status != Cancelled) == TaskStatus.Cancelled' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:61
//    | 
// 60 |         // Test 10: Complex boolean logic
// 61 |         {% if (priority == "High" or priority == "Medium") and status != "Cancelled" %}
//    | 
// warning: Consider using enum value: Instead of '(priority < Medium) == "Medium"', use '(priority < Medium) == Priority.Medium' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:66
//    | 
// 65 |         // Test 11: Ordering comparisons
// 66 |         {% if priority < "Medium" %}
//    | 
// warning: Consider using enum value: Instead of '(status > InProgress) == "InProgress"', use '(status > InProgress) == TaskStatus.InProgress' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:70
//    | 
// 69 |         
// 70 |         {% if status > "InProgress" %}
//    | 
// warning: Consider using enum value: Instead of '(status == Complete) == "Complete"', use '(status == Complete) == TaskStatus.Complete' for better type safety
//   -->  enum/enum_comparison_edge_cases.baml:75
//    | 
// 74 |         // Test 12: Negation
// 75 |         {% if not (status == "Complete") %}
//    | 
