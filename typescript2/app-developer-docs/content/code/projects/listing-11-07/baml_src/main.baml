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

// docs:start excerpt-21
class WebhookNotifier {
    url: string,
}

implements Notifier for WebhookNotifier {
    function send(self, message: string) -> string {
        `webhook to ${self.url}: ${message}`
    }
}
// docs:end excerpt-21


// docs:start excerpt-22
function notify_all(notifiers: Notifier[], message: string) -> string[] throws never {
    notifiers.map((notifier) -> {
        notifier.send(message)
    })
}

function main() -> string[] throws never {
    let notifiers: Notifier[] = [
        EmailNotifier { address: "dev@example.com" },
        SlackNotifier { room: "#deploys" },
        WebhookNotifier { url: "https://example.com/hooks/deploy" },
    ];
    notify_all(notifiers, "Build passed")
}
// docs:end excerpt-22


test "mixed notifier collection" {
    assert.equal(
        main(),
        [
            "email to dev@example.com: Build passed",
            "slack to #deploys: Build passed",
            "webhook to https://example.com/hooks/deploy: Build passed",
        ],
    )
}
