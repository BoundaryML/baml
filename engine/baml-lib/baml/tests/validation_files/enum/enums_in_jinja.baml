enum Status {
    Active @alias("active")
    Inactive @alias("inactive")
    Pending
}

enum Priority {
    High @alias("urgent")
    Low
}

function TestEnumJinjaSyntax(status: Status, priority: Priority, nullable_status: Status?) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        Testing comprehensive enum Jinja syntax constructs:

        // ========= BASIC COMPARISONS =========
        
        // Exact value name matches (should generate warnings)
        {% if status == "Active" %}
            Status is Active (value name)
        {% endif %}
        
        {% if status == "Pending" %}
            Status is Pending (no alias)
        {% endif %}
        
        // Reverse comparisons
        {% if "Inactive" == status %}
            Reverse comparison works
        {% endif %}
        
        // Inequality
        {% if status != "Active" %}
            Status is not Active
        {% endif %}

        // ========= ALIAS COMPARISONS =========
        
        // Comparing to alias (should suggest value name)
        {% if status == "active" %}
            Comparing to alias instead of value
        {% endif %}
        
        {% if priority == "urgent" %}
            Priority alias comparison
        {% endif %}

        // ========= CASE SENSITIVITY =========
        
        // Wrong case (should suggest correct case)
        {% if status == "ACTIVE" %}
            Wrong case comparison
        {% endif %}
        
        {% if priority == "high" %}
            Priority wrong case
        {% endif %}

        // ========= BOOLEAN LOGIC =========
        
        // Multiple enum comparisons with OR
        {% if status == "Active" or status == "Pending" %}
            Status is Active or Pending
        {% endif %}
        
        // Complex boolean expressions
        {% if (status == "Active" or priority == "High") and nullable_status != "Inactive" %}
            Complex boolean condition
        {% endif %}
        
        // Negation
        {% if not (status == "Inactive") %}
            Status is not Inactive
        {% endif %}

        // ========= CONTROL FLOW =========
        
        // If/elif/else chains
        {% if status == "Active" %}
            Active status
        {% elif status == "Pending" %}
            Pending status
        {% elif status == "Inactive" %}
            Inactive status
        {% else %}
            Unknown status
        {% endif %}
        
        // Ternary operators
        Result: {{ "High Priority" if priority == "High" else "Normal Priority" }}
        Status: {{ "Active" if status == "Active" else "Not Active" }}

        // ========= ORDERING COMPARISONS =========
        
        // Less than (should suggest available values)
        {% if status < "ZZZ" %}
            Status comes before ZZZ alphabetically
        {% endif %}
        
        // Greater than
        {% if priority > "AAA" %}
            Priority comes after AAA alphabetically
        {% endif %}
        
        // Mixed ordering
        {% if status >= "Active" and priority <= "Medium" %}
            Mixed ordering comparison
        {% endif %}

        // ========= LOOP CONSTRUCTS =========
        
        // For loop with enum comparison
        {% for s in ["Active", "Inactive", "Pending", "Unknown"] %}
            {% if status == s %}
                Current status matches: {{ s }}
            {% endif %}
        {% endfor %}
        
        // Loop with priority values
        {% for p in ["High", "Medium", "Low"] %}
            {% if priority == p %}
                Priority level: {{ p }}
            {% endif %}
        {% endfor %}

        // ========= CROSS-ENUM TYPE ERRORS =========
        
        // Comparing different enum types (should be type error)
        {% if status == priority %}
            This should generate a cross-enum type error
        {% endif %}

        // ========= DISPLAY FORMATTING =========
        
        // Display should use aliases when available
        Status Display: {{ status }}
        Priority Display: {{ priority }}
        Nullable Display: {{ nullable_status }}

        // ========= EDGE CASES =========
        
        // Typos and fuzzy matching
        {% if status == "Activ" %}
            Typo in status value
        {% endif %}
        
        {% if priority == "Hgh" %}
            Typo in priority value
        {% endif %}
        
        // Non-existent values
        {% if status == "Unknown" %}
            Non-existent status value
        {% endif %}
        
        // Empty string
        {% if status == "" %}
            Empty string comparison
        {% endif %}

        // ========= COMPLEX NESTED CONDITIONS =========
        
        // Deeply nested logic
        {% if (status == "Active" and priority == "High") or (status == "Pending" and priority != "Low") %}
            Complex nested condition
        {% endif %}
        
        // Multiple variable scopes
        {% for item in ["test"] %}
            {% if status == "Active" and item == "test" %}
                Multi-scope comparison
            {% endif %}
        {% endfor %}
    "#
}

// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:20
//    | 
// 19 |         // Exact value name matches (should generate warnings)
// 20 |         {% if status == "Active" %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:24
//    | 
// 23 |         
// 24 |         {% if status == "Pending" %}
//    | 
// warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:29
//    | 
// 28 |         // Reverse comparisons
// 29 |         {% if "Inactive" == status %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:34
//    | 
// 33 |         // Inequality
// 34 |         {% if status != "Active" %}
//    | 
// warning: Use `Status.Active` instead of "active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:41
//    | 
// 40 |         // Comparing to alias (should suggest value name)
// 41 |         {% if status == "active" %}
//    | 
// warning: Did you mean `Priority.High` instead of "urgent" (alias)? Enums are not equal to their alias values.
//   -->  enum/enums_in_jinja.baml:45
//    | 
// 44 |         
// 45 |         {% if priority == "urgent" %}
//    | 
// warning: Use `Status.Active` instead of "ACTIVE" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:52
//    | 
// 51 |         // Wrong case (should suggest correct case)
// 52 |         {% if status == "ACTIVE" %}
//    | 
// warning: Use `Priority.High` instead of "high" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:56
//    | 
// 55 |         
// 56 |         {% if priority == "high" %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:63
//    | 
// 62 |         // Multiple enum comparisons with OR
// 63 |         {% if status == "Active" or status == "Pending" %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:63
//    | 
// 62 |         // Multiple enum comparisons with OR
// 63 |         {% if status == "Active" or status == "Pending" %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:68
//    | 
// 67 |         // Complex boolean expressions
// 68 |         {% if (status == "Active" or priority == "High") and nullable_status != "Inactive" %}
//    | 
// warning: Use `Priority.High` instead of "High" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:68
//    | 
// 67 |         // Complex boolean expressions
// 68 |         {% if (status == "Active" or priority == "High") and nullable_status != "Inactive" %}
//    | 
// warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:68
//    | 
// 67 |         // Complex boolean expressions
// 68 |         {% if (status == "Active" or priority == "High") and nullable_status != "Inactive" %}
//    | 
// warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:73
//    | 
// 72 |         // Negation
// 73 |         {% if not (status == "Inactive") %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:80
//    | 
// 79 |         // If/elif/else chains
// 80 |         {% if status == "Active" %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:82
//    | 
// 81 |             Active status
// 82 |         {% elif status == "Pending" %}
//    | 
// warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:84
//    | 
// 83 |             Pending status
// 84 |         {% elif status == "Inactive" %}
//    | 
// warning: Use `Priority.High` instead of "High" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:91
//    | 
// 90 |         // Ternary operators
// 91 |         Result: {{ "High Priority" if priority == "High" else "Normal Priority" }}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:92
//    | 
// 91 |         Result: {{ "High Priority" if priority == "High" else "Normal Priority" }}
// 92 |         Status: {{ "Active" if status == "Active" else "Not Active" }}
//    | 
// warning: Use one of: Status.Active, Status.Pending - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:97
//    | 
// 96 |         // Less than (should suggest available values)
// 97 |         {% if status < "ZZZ" %}
//    | 
// warning: Use one of: Priority.Low, Priority.High - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:102
//    | 
// 101 |         // Greater than
// 102 |         {% if priority > "AAA" %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:107
//    | 
// 106 |         // Mixed ordering
// 107 |         {% if status >= "Active" and priority <= "Medium" %}
//    | 
// warning: Use one of: Priority.High, Priority.Low - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:107
//    | 
// 106 |         // Mixed ordering
// 107 |         {% if status >= "Active" and priority <= "Medium" %}
//    | 
// warning: Comparing enum Status to string variable - enum-string comparisons will soon be deprecated. Please see https://github.com/BoundaryML/baml/issues/2339.
//   -->  enum/enums_in_jinja.baml:115
//    | 
// 114 |         {% for s in ["Active", "Inactive", "Pending", "Unknown"] %}
// 115 |             {% if status == s %}
//    | 
// warning: Comparing enum Priority to string variable - enum-string comparisons will soon be deprecated. Please see https://github.com/BoundaryML/baml/issues/2339.
//   -->  enum/enums_in_jinja.baml:122
//    | 
// 121 |         {% for p in ["High", "Medium", "Low"] %}
// 122 |             {% if priority == p %}
//    | 
// warning: Use one of: Status.Pending, Status.Inactive - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:130
//    | 
// 129 |         // Comparing different enum types (should be type error)
// 130 |         {% if status == priority %}
//    | 
// warning: Use one of: Status.Active, Status.Inactive - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:144
//    | 
// 143 |         // Typos and fuzzy matching
// 144 |         {% if status == "Activ" %}
//    | 
// warning: Use one of: Priority.High, Priority.Low - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:148
//    | 
// 147 |         
// 148 |         {% if priority == "Hgh" %}
//    | 
// warning: Use one of: Status.Pending, Status.Active - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:153
//    | 
// 152 |         // Non-existent values
// 153 |         {% if status == "Unknown" %}
//    | 
// warning: Use one of: Status.Active, Status.Pending - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:158
//    | 
// 157 |         // Empty string
// 158 |         {% if status == "" %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:165
//    | 
// 164 |         // Deeply nested logic
// 165 |         {% if (status == "Active" and priority == "High") or (status == "Pending" and priority != "Low") %}
//    | 
// warning: Use `Priority.High` instead of "High" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:165
//    | 
// 164 |         // Deeply nested logic
// 165 |         {% if (status == "Active" and priority == "High") or (status == "Pending" and priority != "Low") %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:165
//    | 
// 164 |         // Deeply nested logic
// 165 |         {% if (status == "Active" and priority == "High") or (status == "Pending" and priority != "Low") %}
//    | 
// warning: Use `Priority.Low` instead of "Low" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:165
//    | 
// 164 |         // Deeply nested logic
// 165 |         {% if (status == "Active" and priority == "High") or (status == "Pending" and priority != "Low") %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enums_in_jinja.baml:171
//    | 
// 170 |         {% for item in ["test"] %}
// 171 |             {% if status == "Active" and item == "test" %}
//    | 
