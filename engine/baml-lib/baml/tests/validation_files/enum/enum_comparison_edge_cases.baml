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

client<llm> TestClient {
    provider baml-openai-chat
}

// Test various comparison scenarios that should generate warnings or work correctly
function TestEnumComparisonEdgeCases(priority: Priority, status: TaskStatus) -> string {
    client TestClient
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
    client TestClient
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

generator target {
    output_type "python/pydantic"
}