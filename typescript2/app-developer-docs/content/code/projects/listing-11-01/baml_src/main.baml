class RawDeliveryReceipt {
    route: string,
    status: string,
}

// docs:start excerpt-38
class DeliveryReceipt {
    route: string,
    status: string,

    implements baml.ToString {
        function to_string(self) -> string {
            `${self.status} via ${self.route}`
        }
    }

    implements baml.ToJson {
        function to_json(self) -> baml.json.json {
            { "route": baml.json.from(self.route), "status": baml.json.from(self.status) }
        }
    }
}
// docs:end excerpt-38


function conversion_examples() -> string[] {
    let raw = RawDeliveryReceipt { route: "email", status: "queued" };
    let custom = DeliveryReceipt { route: "email", status: "queued" };
    [raw.to_string(), custom.to_string(), raw.to_json().to_string(), custom.to_json().to_string()]
}

// docs:start excerpt-39
function receipt_representations() -> string[] {
    let receipt = DeliveryReceipt { route: "email", status: "queued" };
    [receipt.to_string(), receipt.to_json().to_string()]
}
// docs:end excerpt-39



interface Destination {
    destination: string

    function display_destination(self) -> string throws never {
        self.destination
    }
}

interface Notifier requires Destination {
    function send(self, message: string) -> string throws never

    function preview(self, message: string) -> string throws never {
        `${self.display_destination()} => ${self.send(message)}`
    }
}

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


function notify(notifier: Notifier, message: string) -> string throws never {
    notifier.preview(message)
}

function notification_examples() -> string[] throws never {
    [
        notify(EmailNotifier { address: "dev@example.com" }, "Build passed"),
        notify(SlackNotifier { room: "#deploys" }, "Build passed"),
        notify(WebhookNotifier { url: "https://example.com/hooks/deploy" }, "Build passed"),
    ]
}

function notify_all(notifiers: Notifier[], message: string) -> string[] throws never {
    notifiers.map((notifier) -> {
        notify(notifier, message)
    })
}

function interface_collection_example() -> string[] throws never {
    let notifiers: Notifier[] = [
        EmailNotifier { address: "dev@example.com" },
        SlackNotifier { room: "#deploys" },
        WebhookNotifier { url: "https://example.com/hooks/deploy" },
    ];
    notify_all(notifiers, "Build passed")
}

// docs:start excerpt-29
function preserve_notifier_type<T extends Notifier>(notifier: T) -> T {
    notifier
}
// docs:end excerpt-29


// docs:start excerpt-28
function erase_notifier_type(notifier: Notifier) -> Notifier {
    notifier
}
// docs:end excerpt-28


// docs:start excerpt-30
function type_examples() -> string[] throws never {
    let email = EmailNotifier { address: "dev@example.com" };

    let erased: Notifier = erase_notifier_type(email);
    let preserved: EmailNotifier = preserve_notifier_type(email);

    [erased.send("Build passed"), preserved.address]
}
// docs:end excerpt-30


function interface_value_examples() -> string[] throws never {
    let email = EmailNotifier { address: "dev@example.com" };
    let slack = SlackNotifier { room: "#deploys" };
    let webhook = WebhookNotifier { url: "https://example.com/hooks/deploy" };
    let notifiers: Notifier[] = [email, slack, webhook];
    let preserved: EmailNotifier = preserve_notifier_type(email);
    let erased: Notifier = erase_notifier_type(webhook);
    [
        notify_all(notifiers, "Build passed").join(" | "),
        preserved.address,
        erased.send("Build passed"),
    ]
}


interface ReceiptStatus {
    function status_label(self) -> string throws never
}

implements ReceiptStatus for DeliveryReceipt {
    function status_label(self) -> string {
        `${self.route}:${self.status}`
    }
}

function label_receipt() -> string throws never {
    let receipt = DeliveryReceipt { route: "email", status: "queued" };
    receipt.status_label()
}


// docs:start excerpt-40
interface PriorityComparison {
    type CompareError
    function compare(self, other: Self) -> int throws Self.CompareError
}

class DeliveryPriority {
    rank: int,
    name: string,

    implements PriorityComparison {
        type CompareError = never

        function compare(self, other: Self) -> int throws never {
            if (self.rank < other.rank) { -1 } else if (self.rank > other.rank) { 1 } else { 0 }
        }
    }
}

function sorted_priorities() -> string[] throws never {
    let priorities = [DeliveryPriority { rank: 2, name: "normal" }, DeliveryPriority { rank: 1, name: "urgent" }];
    priorities.sort_by((left, right) -> { left.compare(right).cmp(0) }).map((priority) -> {
        priority.name
    })
}
// docs:end excerpt-40



function main() -> string[][] {
    [conversion_examples(), notification_examples(), [label_receipt()], sorted_priorities()]
}

test "interface behavior" {
    assert.equal(
        receipt_representations(),
        ["queued via email", "{\"route\": \"email\", \"status\": \"queued\"}"],
    );
    assert.equal(type_examples(), ["email: Build passed", "dev@example.com"]);
    assert.equal(
        conversion_examples(),
        [
            "RawDeliveryReceipt { route: \"email\", status: \"queued\" }",
            "queued via email",
            "{\"route\": \"email\", \"status\": \"queued\"}",
            "{\"route\": \"email\", \"status\": \"queued\"}",
        ],
    );
    assert.equal(
        notification_examples(),
        [
            "dev@example.com => email: Build passed",
            "#deploys => slack: Build passed",
            "https://example.com/hooks/deploy => webhook: Build passed",
        ],
    );
    assert.equal(
        interface_collection_example(),
        [
            "dev@example.com => email: Build passed",
            "#deploys => slack: Build passed",
            "https://example.com/hooks/deploy => webhook: Build passed",
        ],
    );
    assert.equal(
        interface_value_examples(),
        [
            "dev@example.com => email: Build passed | #deploys => slack: Build passed | https://example.com/hooks/deploy => webhook: Build passed",
            "dev@example.com",
            "webhook: Build passed",
        ],
    );
    assert.equal(label_receipt(), "email:queued");
    assert.equal(sorted_priorities(), ["urgent", "normal"])
}
