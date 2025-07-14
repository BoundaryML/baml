class User {
  name string
  is_active bool
}

function FunctionWithConditionals(user: User) -> string {
  client GPT4o
  prompt #"
    {% if user.is_active %}
      Greet the user {{ user.name }}!
    {% else %}
      Tell the user to activate their account
    {% endif %}
  "#
}