enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

function ClassifyMessage { 
  input (message: string, message_date: string)
  output Category[]
}

impl<llm, ClassifyMessage> level1 {
  client $GENERATED_BAML_CLIENT$
  prompt #"
    Classify the following INPUT into following:
    {#print_enum(Category)}

    INPUT
    ---
    date: {#input.message_date}
    message: {#input.message}
    ---

    Output json format:
    {#print_type(output)}

    JSON:
  "#
}
