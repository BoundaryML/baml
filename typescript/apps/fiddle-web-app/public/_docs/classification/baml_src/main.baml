enum MessageType {
  SPAM
  NOT_SPAM
}


function ClassifyText(input: string) -> MessageType {
  client "openai/gpt-4o-mini"
  prompt #"
    Classify the message. 
    {{ ctx.output_format }}

    {{ _.role("user") }} 
    {{ input }}
  "#
}


test BasicSpamTest {
  functions [ClassifyText]
  args {
    input "Buy cheap watches now! Limited time offer!!!"
  }
}

test NonSpamTest {
  functions [ClassifyText]
  args {
    input "Hey Sarah, can we meet at 3 PM tomorrow to discuss the project?"
  }
}
