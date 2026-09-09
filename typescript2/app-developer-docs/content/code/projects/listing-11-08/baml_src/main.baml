// docs:start excerpt-23
interface Destination {
    destination: string
}

class EmailNotifier {
    address: string,

    implements Destination {
        destination as address
    }
}

class SlackNotifier {
    room: string,

    implements Destination {
        destination as room
    }
}

function destinations(values: Destination[]) -> string[] throws never {
    values.map((value) -> {
        value.destination
    })
}
// docs:end excerpt-23


function main() -> string[] throws never {
    destinations([EmailNotifier { address: "dev@example.com" }, SlackNotifier { room: "#deploys" }])
}

test "interface field mappings" {
    assert.equal(main(), ["dev@example.com", "#deploys"])
}
