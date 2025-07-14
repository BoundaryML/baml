class ThreadMessage {
  user_id int
  content string
}

function SummerizeThread(messages: ThreadMessage[]) -> string {
  client ClaudeSonnet
  prompt #"
    {{ _.role('system') }} We are developing a language called BAML and users
    give us feedback in a community chat. You have to summerize conversations
    between users.
    
    {{ _.role('user') }} Summerize the following conversation from our community
    chat as briefly as possible:
    
    {{ messages }}
    
    {{ ctx.output_format }}
  "#
}

test SummerizeThreadTest {
  functions [SummerizeThread]
  args {
    messages [
      { 
        user_id 1
        content  "I think BAML should support recursive types."
      }
      { 
        user_id 2
        content  "Yeah that would be a useful feature."
      }
      { 
        user_id 1
        content  "How long do you think it would take to implement?"
      }
      { 
        user_id 2
        content  "Maybe 3 weeks."
      }
    ] 
  }
}
