// Test nested quote handling in strings
class NestedQuotes {
  single_in_double string @alias("He said 'hello'")
  double_in_single string @alias('She said "hello"')
  escaped_in_escaped string @alias("The \"quote\" is here")
  complex_nesting string @description("It's \"really\" 'complex'")
  json_like string @alias("{\"key\": \"value\"}")
}

// Test quotes in template strings
function FormatMessage(name: string) -> string {
  client GPT4
  prompt #"
    Generate a message for "{{ name }}"
    Include quotes like: "Welcome, '{{ name }}'!"
  "#
}
