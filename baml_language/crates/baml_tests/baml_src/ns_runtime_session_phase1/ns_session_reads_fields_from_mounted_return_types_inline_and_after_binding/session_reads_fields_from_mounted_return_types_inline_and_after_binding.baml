function main() -> string {
    let app = reflect.Package.compile(
        {
            "app.baml": `
                class Ticket { subject string }
                class Envelope { ticket Ticket }
                function GetTicket() -> Ticket { Ticket { subject: "hello" } }
                function GetEnvelope() -> Envelope { Envelope { ticket: GetTicket() } }
            `,
        },
    );
    let s = reflect.Session.new(packages = { "app": app });
    let inline = s.eval(`app.GetTicket().subject`);
    s.eval(`let ticket = app.GetTicket()`);
    let rebound = s.eval(`ticket.subject`);
    let nested = s.eval(`app.GetEnvelope().ticket.subject`);
    `${inline}|${rebound}|${nested}`
}

test "session_reads_fields_from_mounted_return_types_inline_and_after_binding" {
    assert.equal(main(), "hello|hello|hello")
}
