generator Default {
  language python
}

enum MessageSender {
    AI @alias("ai")
    RESIDENT @alias("resident")
}

class Message {
    sender MessageSender @alias("sender1")
    body   string @alias("body1")
} 

class Conversation {
    thread Message
}
  