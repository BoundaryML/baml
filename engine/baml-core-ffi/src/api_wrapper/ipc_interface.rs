use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{self, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::thread::sleep;
use std::time::Duration;

// Assuming _PartialDict and BaseModel structures are analogous to some serde-serializable Rust structs
// These would need to be defined or mapped from existing structs.

#[derive(Serialize, Deserialize)]
struct Message<T> {
  name: String,
  data: T,
}

fn connect_to_server<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
  let mut attempts = 0;
  loop {
    match TcpStream::connect(&addr) {
      Ok(stream) => return Ok(stream),
      Err(e) if attempts < 5 => {
        println!("Connection attempt {} failed: {}", attempts + 1, e);
        sleep(Duration::from_secs(1));
        attempts += 1;
      }
      Err(e) => return Err(e),
    }
  }
}

#[derive(Debug)]
struct TcpIPCChannel {
  socket: TcpStream,
}

impl Clone for TcpIPCChannel {
  fn clone(&self) -> Self {
    Self {
      socket: self.socket.try_clone().unwrap(),
    }
  }
}

impl TcpIPCChannel {
  fn new<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
    let socket = connect_to_server(addr)?;
    Ok(Self { socket })
  }

  fn send<T: Serialize>(&mut self, name: &str, data: &T) -> Result<()> {
    let message = json!({
      "name": name,
      "data": data,
    })
    .to_string();
    self
      .socket
      .write_all(message.as_bytes())
      .map_err(|e| e.into())
  }
}

#[derive(Debug, Clone)]
pub struct IPCChannel {
  channel: Option<TcpIPCChannel>,
}

impl IPCChannel {
  pub fn new<A: ToSocketAddrs>(addr: Option<A>) -> Result<Self> {
    let channel = match addr {
      Some(addr) => Some(TcpIPCChannel::new(addr)?),
      None => None,
    };

    Ok(Self { channel })
  }

  pub fn send<T: Serialize>(&mut self, name: &str, data: T) -> Result<()> {
    self
      .channel
      .as_mut()
      .map(|c| c.send(name, &data))
      .transpose()?;

    Ok(())
  }
}
