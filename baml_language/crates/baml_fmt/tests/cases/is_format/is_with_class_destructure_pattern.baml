class User {
    name: string,
    age: int,
}

function is_user(u: User) -> bool {
    u is User { name, age }
}
