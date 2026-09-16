// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0132
// docs:endmeta
// docs:start excerpt-43
interface RouteLabel {
    function route_label(self) -> string throws never
}

class EmailNotifier {
    address: string,

    implements RouteLabel {
        function route_label(self) -> string {
            self.address
        }
    }
}

implements RouteLabel for EmailNotifier {
    function route_label(self) -> string {
        `email:${self.address}`
    }
}
// docs:end excerpt-43
