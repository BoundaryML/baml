generator Default {
  language python
}

enum MessageSender {
    AI
    RESIDENT
}

class Message {
    sender MessageSender
    body   string
}

class Conversation {
    thread Message[]
}
