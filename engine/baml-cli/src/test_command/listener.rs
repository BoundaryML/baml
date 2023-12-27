use std::io::{BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    loop {
        match reader.read_line(&mut line) {
            Ok(0) => {
                // End of stream
                break;
            }
            Ok(_) => {
                if line.ends_with("<END_MSG>\n") {
                    println!("Received: {}", line);
                    line.clear(); // Clear the buffer for the next message
                }
            }
            Err(e) => {
                eprintln!("Failed to read from socket: {}", e);
                break;
            }
        }
    }
}

pub fn start_server() -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}
