// docs:start excerpt-37
interface CustomerLabel {
    function label(self) -> string throws never
}

interface AuditLabel {
    function label(self) -> string throws never
}

class EmailNotifier {
    address: string,

    implements CustomerLabel {
        function label(self) -> string {
            self.address
        }
    }

    implements AuditLabel {
        function label(self) -> string {
            `email:${self.address}`
        }
    }
}

function notifier_labels() -> string[] {
    let notifier = EmailNotifier { address: "dev@example.com" };
    [notifier.as<CustomerLabel>.label(), notifier.as<AuditLabel>.label()]
}
// docs:end excerpt-37



test "explicit interface selection chooses the intended label" {
    assert.equal(notifier_labels(), ["dev@example.com", "email:dev@example.com"]);
}
