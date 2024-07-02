// You can improve results by aliasing field names to symbols like "k1" ... "kN".
// This makes the LLM pay attention to your descriptions instead of just the enum or property name, since those can introduce biases. See paper: https://arxiv.org/abs/2305.08298 for more details.

// Check the prompt preview to see that aliases are being sent.

// When you add aliases you don't need to change your resulting python code. Your models
// stay the same. They are merely a way to aid in prompt engineering.
enum Category {
    Refund @alias("k1")
    @description("Customer wants to refund a product")

    CancelOrder @alias("k2")
    @description("Customer wants to cancel an order")

    TechnicalSupport @alias("k3")
    @description("Customer needs help with a technical issue unrelated to account creation or login")

    AccountIssue @alias("k4")
    @description("Specifically relates to account-login or account-creation")

    Question @alias("k5")
    @description("Customer has a question")
}

function ClassifyMessageWithSymbol(input: string) -> Category {
  client GPT4o

  prompt #"
    Classify the following INPUT into ONE
    of the following categories:

    INPUT: {{ input }}

    {{ ctx.output_format }}

    Response:
  "#
}

test Test1 {
  functions [ClassifyMessageWithSymbol]
  args {
    input "I can't access my account using my login credentials. I havent received the promised reset password email. Please help."
  }
}