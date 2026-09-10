// docs:start excerpt-31
interface BatchSize {
    function batch_size(self) -> int throws never
}
// docs:end excerpt-31


// docs:start excerpt-32
implements<T> BatchSize for T[] {
    function batch_size(self) -> int {
        self.length()
    }
}
// docs:end excerpt-32


function blanket_examples() -> int[] {
    [["email", "slack", "webhook"].batch_size(), ["primary", "backup"].batch_size()]
}

// docs:start excerpt-33
function batch_sizes() -> int[] {
    [["email", "slack", "webhook"].batch_size(), [1, 2].batch_size()]
}
// docs:end excerpt-33


interface Notifier {
    function send(self, message: string) -> string throws never
}

// docs:start excerpt-34
interface NotificationBatch {
    function send_all(self, message: string) -> string throws never
}
// docs:end excerpt-34


// docs:start excerpt-35
implements<T extends Notifier> NotificationBatch for T[] {
    function send_all(self, message: string) -> string {
        self.map((notifier) -> { notifier.send(message) }).join(" | ")
    }
}
// docs:end excerpt-35


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

function bounded_blanket_example() -> string {
    [EmailNotifier { address: "dev@example.com" }, EmailNotifier { address: "ops@example.com" },]
        .send_all("Build passed")
}

// docs:start excerpt-36
function notify_email_batch() -> string {
    [EmailNotifier { address: "dev@example.com" }, EmailNotifier { address: "ops@example.com" },]
        .send_all("Build passed")
}
// docs:end excerpt-36


test "blanket implementations" {
    assert.equal(batch_sizes(), [3, 2]);
    assert.equal(
        notify_email_batch(),
        "email to dev@example.com: Build passed | email to ops@example.com: Build passed",
    )
}
