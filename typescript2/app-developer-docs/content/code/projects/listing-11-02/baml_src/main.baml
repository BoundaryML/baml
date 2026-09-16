// docs:start excerpt-20
interface Notifier {
    function send(self, message: string) -> string throws never
}

class EmailNotifier {
    address: string,

    implements Notifier {
        function send(self, message: string) -> string {
            `email to ${self.address}: ${message}`
        }
    }
}

class SlackNotifier {
    room: string,

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
