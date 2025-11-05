// Class constructor test

class Message {
  sender "user" | "agent"
  content string
}

function CreateMessage() -> Message {
  Message {
    sender: "agent",
    content: "Hello!"
  }
}

//> CreateMessage()
//
// {
//   "sender": "agent",
//   "content": "Hello!"
// }
