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

function TestEnumStringComparison(status: Status, category: Category) -> string {
    client "openai/gpt-4o-mini"
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
    client "openai/gpt-4o-mini"
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

// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:19
//    | 
// 18 |         // These should work - comparing to value name
// 19 |         {% if status == "Active" %}
//    | 
// warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:23
//    | 
// 22 |         
// 23 |         {% if "Inactive" == status %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:27
//    | 
// 26 |         
// 27 |         {% if status != "Pending" %}
//    | 
// warning: Use `Status.Active` instead of "active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:32
//    | 
// 31 |         // These compare to alias - should NOT match
// 32 |         {% if status == "active" %}
//    | 
// warning: Use `Category.Refund` instead of "gimmie" (alias) - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:36
//    | 
// 35 |         
// 36 |         {% if category == "gimmie" %}
//    | 
// warning: Use one of: Status.Pending, Status.Active - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:41
//    | 
// 40 |         // Test ordering comparisons
// 41 |         {% if status < "ZZZ" %}
//    | 
// warning: Use one of: Status.Active, Status.Inactive - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:45
//    | 
// 44 |         
// 45 |         {% if status > "AAA" %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:59
//    | 
// 58 |     prompt #"
// 59 |         {% if status == "Active" or status == "Pending" %}
//    | 
// warning: Use `Status.Pending` instead of "Pending" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:59
//    | 
// 58 |     prompt #"
// 59 |         {% if status == "Active" or status == "Pending" %}
//    | 
// warning: Use `Status.Inactive` instead of "Inactive" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:61
//    | 
// 60 |             Status is either Active or Pending
// 61 |         {% elif status == "Inactive" %}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:68
//    | 
// 67 |         // Test in ternary operator
// 68 |         Result: {{ "Active" if status == "Active" else "Not Active" }}
//    | 
// warning: Use `Status.Active` instead of "Active" - comparing enums with strings will soon be deprecated.
//   -->  enum/enum_string_comparison.baml:71
//    | 
// 70 |         // Test complex conditions
// 71 |         {% if status == "Active" and category != "Refund" %}
//    | 
// warning: Variable `category` does not exist. Did you mean one of these: `_`, `ctx`, `status`?
//   -->  enum/enum_string_comparison.baml:71
//    | 
// 70 |         // Test complex conditions
// 71 |         {% if status == "Active" and category != "Refund" %}
//    | 
