enum MessageType {
  FeatureRequest
  BugReport
  Question
  Uncategorized
}

class Message {
  id int
  content string
}

class Classification {
  message_id int
  message_type MessageType
}

function ClassifyMessages(messages: Message[]) -> Classification[] {
  client ClaudeSonnet
  prompt #"
    {{ _.role('system') }} We are developing a language called BAML and users
    give us feedback in a community chat. You have to classify the messages we
    receive from users.
    
    {{ _.role('user') }} Classify the following messages from users into one of
    the given categories:
    
    {{ messages }}
    
    {{ ctx.output_format }}
  "#
}

test ClassifyMessageTest {
  functions [ClassifyMessages]
  args {
    messages [
      { 
        id 1
        content  "BAML should support recursive types."
      }
      { 
        id 2
        content  "I found a bug in the BAML compiler."
      }
      { 
        id 3
        content  "How do I install the BAML compiler?"
      }
      { 
        id 4
        content  "I like BAML."
      }
    ] 
  }
}
