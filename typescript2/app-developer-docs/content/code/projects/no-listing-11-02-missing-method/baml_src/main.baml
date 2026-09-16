// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0113
// docs:endmeta
interface Notifier {
    function send(self, message: string) -> string throws never
}

class EmailNotifier {
    address: string,

    implements Notifier {}
}
