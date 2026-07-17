// Tests for `baml.net` (TcpStream / TcpListener / UdpSocket).
//
// Each case is a top-level helper `function` (test-block local-boxing VM bug)
// plus a `test` block that asserts on its result.
//
// For tests that need a listening TCP socket: `baml.http.Server.bind("127.0.0.1:0")`
// supplies one. It exposes `.addr`, and its socket queues connections from bind
// time even before `serve` runs (see ns_http_server/http_server.baml), so a raw
// `baml.net.TcpStream.connect` against it succeeds while the peer stays silent.
// `baml.net` itself has no `local_addr`, so a BAML-side raw TCP/UDP peer cannot
// be bootstrapped.
//
// Error classes (crates/sys_native/src/io_impls.rs):
//   read / recv_from deadline expiry          -> baml.errors.Timeout
//   op on a closed stream / listener / socket -> baml.errors.Io
//
// The TCP read-timeout test lives in Rust (tests/net.rs), not here: its silent
// peer needs a controlled listener that stays alive across the throwing read,
// which a GC-managed BAML `baml.http.Server` cannot guarantee under load.

// ─── close() invalidates the shared handle ────────────────────────────────────

// Connect succeeds, then close() invalidates the shared handle so a subsequent
// read() on the same socket fails deterministically ("TcpStream is closed").
function net_tcp_read_after_close_errors_fn() -> uint8array {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let sock = baml.net.TcpStream.connect(server.addr);
    sock.close();
    sock.read()
}

test "net_tcp_read_after_close_errors" {
    {
        net_tcp_read_after_close_errors_fn();
        baml.sys.panic("read after close should fail")
    } catch (e) {
        baml.errors.Io => null
    }
}

function net_tcp_listener_accept_after_close_errors_fn() -> uint8array {
    let listener = baml.net.TcpListener.bind("127.0.0.1:0");
    listener.close();
    let sock = listener.accept();
    sock.read()
}

test "net_tcp_listener_accept_after_close_errors" {
    {
        net_tcp_listener_accept_after_close_errors_fn();
        baml.sys.panic("accept after close should fail")
    } catch (e) {
        baml.errors.Io => null
    }
}

function net_udp_send_after_close_errors_fn() -> int {
    let sock = baml.net.UdpSocket.bind("127.0.0.1:0");
    sock.close();
    sock.send_to("x".to_utf8(), "127.0.0.1:9")
}

test "net_udp_send_after_close_errors" {
    {
        net_udp_send_after_close_errors_fn();
        baml.sys.panic("send_to after close should fail")
    } catch (e) {
        baml.errors.Io => null
    }
}

// ─── UDP recv timeout ─────────────────────────────────────────────────────────

// A bound socket with nobody sending: recv_from(timeout=50ms) must surface as
// baml.errors.Timeout rather than blocking forever.
function net_udp_recv_timeout_fires_fn() -> uint8array {
    let sock = baml.net.UdpSocket.bind("127.0.0.1:0");
    let dgram = sock.recv_from(timeout = baml.time.Duration.from_milliseconds(50n));
    dgram.data
}

test "net_udp_recv_timeout_fires" {
    {
        net_udp_recv_timeout_fires_fn();
        baml.sys.panic("recv_from with a 50ms timeout and no datagram should time out")
    } catch (e) {
        baml.errors.Timeout => null
    }
}
