class Agent {
    client: string?,
}

function make(client: string?) -> Agent {
    let a = Agent { client: client };
    a
}
