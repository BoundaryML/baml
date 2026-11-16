// `@trace.redact` and `@@trace.redact`: telemetry records a redaction in
// place of the value. The program behaves the same; the recording is checked
// by `bex_engine/tests/redaction_recording.rs`, since BAML cannot read it.

class TraceRedactCredentials {
    key: string,
    @@trace.redact
}

class TraceRedactClient {
    name: string,
    token: string @trace.redact,
    credentials: TraceRedactCredentials,
}

function trace_redact_log() -> string {
    let account = TraceRedactClient {
        name: "visible-name",
        token: "secret-in-a-redacted-field",
        credentials: TraceRedactCredentials { key: "secret-in-a-redacted-class" },
    };
    log.info(account, event_name = "client");
    account.token + account.credentials.key
}

test "redaction_leaves_values_unchanged" {
    assert.equal(trace_redact_log(), "secret-in-a-redacted-fieldsecret-in-a-redacted-class")
}

// A class built at run time from a redacted one keeps the redaction of the
// fields it copies.
function trace_redact_copied_log() -> string {
    let source = TraceRedactClient.type().as_class() ?? baml.sys.panic("missing class");
    let copy = reflect.Class.builder("TraceRedactCopy").fields_from(source).build();
    let type T = copy.as_type();
    let value = reflect.Class.instantiate<T>({
        "name": "visible-name",
        "token": "secret-in-a-copied-field",
        "credentials": TraceRedactCredentials { key: "secret-in-a-redacted-class" },
    });
    log.info(value, event_name = "copy");
    "copied"
}

test "a_copied_redaction_leaves_values_unchanged" {
    assert.equal(trace_redact_copied_log(), "copied")
}
