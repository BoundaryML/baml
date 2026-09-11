// docs:start excerpt-20
interface Notifier {
    function send(self, message: string) -> string throws never
}

class EmailNotifier {
    address: string,

    implements Destination { destination as address }

    implements Notifier {
        function send(self, message: string) -> string {
            `email to ${self.address}: ${message}`
        }
    }
}

class SlackNotifier {
    room: string,

    implements Destination { destination as room }

    implements Notifier {
        function send(self, message: string) -> string {
            `slack to ${self.room}: ${message}`
        }
    }
}

function notify(notifier: Notifier, message: string) -> string throws never {
    notifier.send(message)
}
// docs:end excerpt-20


function main() -> string[] throws never {
    [
        notify(EmailNotifier { address: "dev@example.com" }, "Build passed"),
        notify(SlackNotifier { room: "#deploys" }, "Build passed"),
    ]
}

test "interface value" {
    assert.equal(
        main(),
        ["email to dev@example.com: Build passed", "slack to #deploys: Build passed"],
    )
}

interface Destination { destination: string }
function destinations(values: Destination[]) -> string[] throws never {
    values.map((value) -> { value.destination })
}
test "adding field mappings retains notification behavior" {
    let email = EmailNotifier { address: "dev@example.com" };
    let slack = SlackNotifier { room: "#deploys" };
    assert.equal(destinations([email, slack]), ["dev@example.com", "#deploys"]);
    assert.equal(notify(email, "Build passed"), "email to dev@example.com: Build passed");
    assert.equal(notify(slack, "Build passed"), "slack to #deploys: Build passed");
}
