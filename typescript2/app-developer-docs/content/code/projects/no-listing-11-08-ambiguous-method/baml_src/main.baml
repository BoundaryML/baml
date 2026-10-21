// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0121
// docs:endmeta
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

function ambiguous_label() -> string {
    EmailNotifier { address: "dev@example.com" }.label()
}
