enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

function ClassifyMessage(input: string) -> Category {
  client "openai/gpt-4o-mini"
  prompt #"
    Classify the INPUT.

    {{ ctx.output_format }}

    INPUT:
    ---
    {{ input }}
    ---
  "#
}

test Test1 {
  functions [ClassifyMessage]
  args {
    // input is the first argument of ClassifyMessage
    input "Can't access my account using my usual login credentials, and each attempt results in an error message stating 'Invalid username or password.' I have tried resetting my password using the 'Forgot Password' link, but I haven't received the promised password reset email."
  }
  @@assert( {{ this == "AccountIssue" }})
}