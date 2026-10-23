//! Delivery uses the proxy the environment names, like BAML's other HTTP
//! clients. A file of its own: the test sets process-wide proxy variables.
#![allow(unsafe_code)]

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use btel_bcs::delivery::{BcsDelivery, DeliveryConfig, DeliveryError, InitialAuthorization};
use btel_recorder::RecordingId;

/// A forward proxy that answers every request itself with 204 and records
/// each request line.
fn proxy() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let lines = seen.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap();
                    }
                }
            }
            reader.read_exact(&mut vec![0; length]).unwrap();
            lines.lock().unwrap().push(line.trim_end().to_owned());
            stream
                .write_all(
                    b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                )
                .unwrap();
        }
    });
    (address, seen)
}

fn finish_first_connection(endpoint: &str) -> (Result<(), DeliveryError>, InitialAuthorization) {
    let delivery = BcsDelivery::new(
        DeliveryConfig {
            initial_recording_id: Some(RecordingId::generate()),
            terminal_initial_connection: true,
            max_attempts: 1,
            request_timeout: Duration::from_secs(5),
            ..DeliveryConfig::new(endpoint.parse().unwrap())
        },
        |_| {},
    )
    .unwrap();
    let result = delivery.finish();
    (result, delivery.initial_authorization())
}

#[test]
fn delivery_uses_the_environment_proxy_unless_no_proxy_names_the_host() {
    let (proxy, seen) = proxy();
    // No other test runs in this process, and the proxy thread never reads
    // the environment.
    unsafe {
        for name in [
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
            "http_proxy",
            "no_proxy",
            "REQUEST_METHOD",
        ] {
            std::env::remove_var(name);
        }
        std::env::set_var("HTTP_PROXY", &proxy);
        std::env::set_var("NO_PROXY", "127.0.0.1");
    }
    // Nothing listens on port 9: only the proxy can answer.
    let (result, authorization) = finish_first_connection("http://localhost:9");
    assert_eq!(result, Ok(()));
    assert_eq!(authorization, InitialAuthorization::Authorized);
    let lines = seen.lock().unwrap().clone();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("POST http://localhost:9/v1/recordings/")
            && lines[0].ends_with("/heartbeat HTTP/1.1"),
        "{lines:?}"
    );
    // NO_PROXY names this host: delivery connects directly, and is refused.
    let (result, authorization) = finish_first_connection("http://127.0.0.1:9");
    assert!(
        matches!(result, Err(DeliveryError::InitialConnection(_))),
        "{result:?}"
    );
    assert!(matches!(authorization, InitialAuthorization::Failed(_)));
    assert_eq!(seen.lock().unwrap().len(), 1);
}
