function GetUser(id: int) -> User {
    client GPT4
    prompt #"Get user {{id}}"#
}

function GetStatus() -> Status {
    client GPT4
    prompt #"Get status"#
}