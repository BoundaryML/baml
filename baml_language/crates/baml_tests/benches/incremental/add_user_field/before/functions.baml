function GetUser(id: string) -> User {
    client GPT4
    prompt #"Get user {{id}}"#
}

function GetCompany(name: string) -> Company {
    client GPT4
    prompt #"Get company {{name}}"#
}