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

// This type is owned by another package.
class WebhookNotifier {
    url: string,
}

implements Notifier for WebhookNotifier {
    function send(self, message: string) -> string {
        `webhook to ${self.url}: ${message}`
    }
}

function notify(notifier: Notifier, message: string) -> string throws never {
    notifier.send(message)
}

function main() -> string[] throws never {
    [
        notify(EmailNotifier { address: "dev@example.com" }, "Build passed"),
        notify(SlackNotifier { room: "#deploys" }, "Build passed"),
        notify(WebhookNotifier { url: "https://example.com/hooks/deploy" }, "Build passed"),
    ]
}

test "a third route changes no consumer" {
    assert.equal(
        main(),
        [
            "email to dev@example.com: Build passed",
            "slack to #deploys: Build passed",
            "webhook to https://example.com/hooks/deploy: Build passed",
        ],
    )
}
