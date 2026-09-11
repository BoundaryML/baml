// docs:start excerpt-19
class EmailNotifier {
    address: string,

    function send(self, message: string) -> string {
        `email to ${self.address}: ${message}`
    }
}

class SlackNotifier {
    room: string,

    function send(self, message: string) -> string {
        `slack to ${self.room}: ${message}`
    }
}

type Notifier = EmailNotifier | SlackNotifier;

function notify(notifier: Notifier, message: string) -> string {
    match (notifier) {
        let email: EmailNotifier => email.send(message),
        let slack: SlackNotifier => slack.send(message),
    }
}
// docs:end excerpt-19


function main() -> string[] {
    [
        notify(EmailNotifier { address: "dev@example.com" }, "Build passed"),
        notify(SlackNotifier { room: "#deploys" }, "Build passed"),
    ]
}

test "union notification" {
    assert.equal(
        main(),
        ["email to dev@example.com: Build passed", "slack to #deploys: Build passed"],
    )
}
