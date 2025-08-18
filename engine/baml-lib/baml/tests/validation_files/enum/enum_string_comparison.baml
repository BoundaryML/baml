enum Status {
    Active @alias("active")
    Inactive @alias("inactive")
    Pending
}

enum Category {
    Refund @alias("gimmie")
    Payment @alias("payment")
    Adjustment
}

client<llm> TestClient {
    provider baml-openai-chat
}

function TestEnumStringComparison(status: Status, category: Category) -> string {
    client TestClient
    prompt #"
        Testing enum to string comparisons:
        
        // These should work - comparing to value name
        {% if status == "Active" %}
            Status is Active (value name comparison)
        {% endif %}
        
        {% if "Inactive" == status %}
            Status is Inactive (reverse comparison)
        {% endif %}
        
        {% if status != "Pending" %}
            Status is not Pending
        {% endif %}
        
        // These compare to alias - should NOT match
        {% if status == "active" %}
            This should not match - comparing to alias
        {% endif %}
        
        {% if category == "gimmie" %}
            This should not match - comparing to alias
        {% endif %}
        
        // Test ordering comparisons
        {% if status < "ZZZ" %}
            Status comes before ZZZ
        {% endif %}
        
        {% if status > "AAA" %}
            Status comes after AAA
        {% endif %}
        

        // Test display uses alias
        Status display: {{ status }}
        Category display: {{ category }}
    "#
}

function TestEnumComparisonsInConditions(status: Status) -> string {
    client TestClient
    prompt #"
        {% if status == "Active" or status == "Pending" %}
            Status is either Active or Pending
        {% elif status == "Inactive" %}
            Status is Inactive
        {% else %}
            Unknown status
        {% endif %}
        
        // Test in ternary operator
        Result: {{ "Active" if status == "Active" else "Not Active" }}
        
        // Test complex conditions
        {% if status == "Active" and category != "Refund" %}
            Active and not a refund
        {% endif %}
    "#
}

generator target {
    output_type "python/pydantic"
}

// warning: Type mismatch: '(status == Active)' compares values of different types (enum Status and literal["Active"]). Starting in baml 0.206.0, strings are not implicitly converted to enum values (e.g. you should use `MyEnum.VALUE_A` instead of `"VALUE_A"`).
//   -->  enum/enum_string_comparison.baml:72
//    | 
// 71 |         // Test in ternary operator
// 72 |         Result: {{ "Active" if status == "Active" else "Not Active" }}
//    | 
