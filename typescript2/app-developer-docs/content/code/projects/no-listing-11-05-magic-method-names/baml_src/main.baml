// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0140
//     - code: E0142
// docs:endmeta
class DeliveryReceipt {
    route: string,
    status: string,

    function to_string(self) -> string {
        "queued via email"
    }

    function to_json(self) -> baml.json.json {
        { "route": baml.json.from(self.route), "status": baml.json.from(self.status) }
    }
}
