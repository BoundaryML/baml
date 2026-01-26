class User {
    id: string
    name: string
    email: string
    created_at: string
}

class Message {
    id: string
    content: string
    author: User
}

class Thread {
    id: string
    participants: User[]
    messages: Message[]
}