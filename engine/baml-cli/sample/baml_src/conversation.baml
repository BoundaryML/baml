// These are some helper types for the chatbot example.


enum UserType {
    AI
    User
}

class Message {
    user UserType
    content string

    // In baml, you can define properties that are computed from other properties.
    {//
    This will get transpiled to:
    python#"
        @property
        def display(self):
            if self.user == UserType.AI:
                return f"Service Provider: {self.content}"
            else:
                return f"User: {self.content}"
    "#
    //}
    display string @get(
        python#"
            if self.user == UserType.AI:
                return f"Service Provider: {self.content}"
            else:
                return f"User: {self.content}"
        "#
    )
}

class Conversation {
    messages Message[]

    display string @get(
        python#"
            return "\n".join([m.display for m in self.messages])
        "#
    )
}
