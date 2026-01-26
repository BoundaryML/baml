function GetUser(id: string) -> User {
    client GPT4
    prompt #"Fetch user with id: {{id}}"#
}

function CreateMessage(content: string, author: User) -> Message {
    client GPT4
    prompt #"
        Create a message with:
        Content: {{content}}
        Author: {{author}}
    "#
}

function GetThread(id: string) -> Thread {
    client GPT4
    prompt #"Get thread {{id}} with all participants and messages"#
}