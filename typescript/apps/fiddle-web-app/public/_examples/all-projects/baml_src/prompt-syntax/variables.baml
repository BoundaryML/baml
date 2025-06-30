// We can define other "template string" variables that can be used in a prompt.
class UserMessage {
  message string
  role string
}

// prompt strings and template strings use Jinja2 syntax: https://jinja.palletsprojects.com/en/3.1.x/templates/
template_string PrintMessages(messages: UserMessage[]) #"
  {% for m in messages %}
    {{ _.role(m.role) }}
    {{ m.message }}
  {% endfor %}
"#

function ClassifyConversation(messages: UserMessage[]) -> Category[] {
  client GPT4o
  prompt #"
    Classify this conversation:
    {{ PrintMessages(messages) }}

    Use the following categories:
    {{ ctx.output_format}}
  "#
}

test TestClassifyConvo {
  functions [ClassifyConversation]
  args {
    messages [
      {
        role "user"
        message "I can't access my account using my login credentials. I havent received the promised reset password email. Please help."
      }
      {
        role "assistant"
        message "I'm sorry to hear that. Let me help you with that."
      }
      {
        role "user"
        message "Thank you."
      }
    ]
  }
}
