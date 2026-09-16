// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0170
// docs:endmeta
interface Notifier {
    function send(self, message: string) -> string
}
