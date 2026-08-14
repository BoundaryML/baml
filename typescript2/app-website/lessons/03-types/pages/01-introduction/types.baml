function UseSomeTypes(
    exact_age: float,
    name: string,
    role: "user" | "admin",
    uncle_name: string?,
    friend_birthdays: map<string, string>
) -> int[] {
  client GPT4
  prompt #"
    Pick some winning lottery numbers for {{ name }}.
    Their age is {{ exact_age }}, and they are a {{ role }}.

    {% if uncle_name %}
      They have an uncle named {{ uncle_name }}.
    {% endif %}

    Their friends' birthdays:
    {{ friend_birthdays }}

    The numbers should be between 1 and 100 inclusive.

    {{ ctx.output_format }}
  "#
}