class Person {  // Renamed from User
    id: string
    name: string
    email: string
    created_at: string
}

class Message {
    id: string
    content: string
    author: Person  // Updated reference
}

class Thread {
    id: string
    participants: Person[]  // Updated reference
    messages: Message[]
}