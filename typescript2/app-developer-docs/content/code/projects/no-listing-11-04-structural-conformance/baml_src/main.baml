// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
//       messageContains: "expected `Destination`, found `EmailNotifier`"
// docs:endmeta
// docs:start excerpt-41
interface Destination {
    destination: string
}

class EmailNotifier {
    destination: string,
}

function show_destination(value: Destination) -> string throws never {
    value.destination
}

function main() -> string throws never {
    show_destination(EmailNotifier { destination: "dev@example.com" })
}
// docs:end excerpt-41
