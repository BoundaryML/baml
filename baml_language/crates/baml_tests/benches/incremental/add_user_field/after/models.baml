class User {
    id: string
    name: string
    email: string
    phone?: string      // Added field
    verified: bool      // Added field
}

class Company {
    name: string
    users: User[]
}