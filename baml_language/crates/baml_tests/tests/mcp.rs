//! MCP connections own their subprocess pipes independently of the process handle.

use baml_tests::baml_test;
use bex_external_types::BexExternalValue;

#[tokio::test]
/// Retaining a closed connection must not retain usable process pipes.
async fn closing_a_connection_releases_pipes_after_its_process_handle_is_closed() {
    let (program, args) = if cfg!(windows) {
        ("cmd", r#"["/c", "more"]"#)
    } else {
        ("sh", r#"["-c", "cat"]"#)
    };
    let output = baml_test!(&format!(
        r#"
        function main() -> bool {{
            let child = baml.sys.subprocess("{program}", args = {args}, stdin = "pipe", stdout = "pipe");
            let input = child.stdin ?? baml.sys.panic("missing stdin");
            let output = child.stdout ?? baml.sys.panic("missing stdout");
            let connection = ai.mcp.Connection {{
                server_name: "fixture", p: child, out: output.lines(),
                timeout_ms: 1000, next_id: 1, _closed: false,
            }};
            child.close();
            let _ = {{ connection.close(); true }} catch (e) {{ baml.errors.Io => false }};
            connection.close();
            let write_closed = {{ input.write(b"unexpected"); false }} catch (e) {{ baml.errors.Io => true }};
            let read_closed = {{ output.read(1); false }} catch (e) {{ baml.errors.Io => true }};
            write_closed && read_closed
        }}
        "#,
    ));
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
