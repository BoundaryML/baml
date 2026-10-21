// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0007
// docs:endmeta
interface Notifier {
    function send(self, message: string) -> string throws never
}

interface NotificationBatch {
    function send_all(self, message: string) -> string throws never
}

implements<T extends Notifier> NotificationBatch for T[] {
    function send_all(self, message: string) -> string {
        self.map((notifier) -> { notifier.send(message) }).join(" | ")
    }
}

function invalid_join() -> string {
    [1, 2].send_all("Build passed")
}
