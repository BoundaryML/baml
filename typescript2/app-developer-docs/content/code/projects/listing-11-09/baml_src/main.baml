// docs:start excerpt-24
interface Destination {
    destination: string
}
// docs:end excerpt-24


// docs:start excerpt-25
interface Notifier requires Destination {
    function send(self, message: string) -> string throws never

    function preview(self, message: string) -> string throws never {
        `${self.destination} => ${self.send(message)}`
    }
}
// docs:end excerpt-25


// docs:start excerpt-26
class EmailNotifier {
    address: string,

    implements Destination {
        destination as address
    }

    implements Notifier {
        function send(self, message: string) -> string {
            `email: ${message}`
        }
    }
}
// docs:end excerpt-26


class SlackNotifier {
    room: string,

    implements Destination {
        destination as room
    }

    implements Notifier {
        function send(self, message: string) -> string {
            `slack: ${message}`
        }
    }
}

class WebhookNotifier {
    url: string,

    implements Destination {
        destination as url
    }

    implements Notifier {
        function send(self, message: string) -> string {
            `webhook: ${message}`
        }
    }
}

function preview_all(notifiers: Notifier[], message: string) -> string[] throws never {
    notifiers.map((notifier) -> {
        notifier.preview(message)
    })
}

// docs:start excerpt-27
function default_email_example() -> string throws never {
    EmailNotifier { address: "dev@example.com" }.preview("Build passed")
}
// docs:end excerpt-27


function main() -> string[] throws never {
    let notifiers: Notifier[] = [
        EmailNotifier { address: "dev@example.com" },
        SlackNotifier { room: "#deploys" },
        WebhookNotifier { url: "https://example.com/hooks/deploy" },
    ];
    preview_all(notifiers, "Build passed")
}

test "required and default behavior" {
    assert.equal(default_email_example(), "dev@example.com => email: Build passed");
    assert.equal(
        main(),
        [
            "dev@example.com => email: Build passed",
            "#deploys => slack: Build passed",
            "https://example.com/hooks/deploy => webhook: Build passed",
        ],
    )
}
