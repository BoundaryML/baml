enum Category3 {
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

function ClassifyMessage3(input: string) -> Category {
  client GPT4

  prompt #"
    Classify the following INPUT into ONE
    of the following categories:

    INPUT: {{ input }}

    {{ ctx.output_format }}

    Response:
  "#
}