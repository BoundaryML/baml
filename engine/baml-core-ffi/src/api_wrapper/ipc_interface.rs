use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::thread::sleep;
use std::time::Duration;


use super::core_types::{LogSchema, UpdateTestCase};

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
    .to_string()
      + "<BAML_END_MSG>\n";
    // %m:%s.%3N time
    // let now = chrono::Local::now().format("%H:%M:%S%.3f");
    // println!("{now} Sending message: {}", name);
    self.socket.write_all(message.as_bytes())?;
    // println!("{now} Sent message: {}", name);
    self
      .socket
      .flush()
      .map_err(|e| anyhow::format_err!("Failed to flush message: {}", e))?;
    // println!("{now} Flushed message: {}", name);
    Ok(())
  }
}

#[derive(Debug, Clone)]
pub struct IPCChannel {
  channel: Option<std::sync::Arc<napi::tokio::sync::Mutex<TcpIPCChannel>>>,
}

impl IPCChannel {
  pub fn new<A: ToSocketAddrs>(addr: Option<A>) -> Result<Self> {
    let channel = match addr {
      Some(addr) => Some(TcpIPCChannel::new(addr)?),
      None => None,
    };

    let channel = channel.map(|c| std::sync::Arc::new(napi::tokio::sync::Mutex::new(c)));

    Ok(Self { channel })
  }

  pub async fn send(&self, message: IPCMessage<'_>) -> Result<()> {
    match self.channel {
      Some(ref c) => {
        let mut c = c.lock().await;
        c.send(message.name(), &message.data())?;
        Ok(())
      }
      None => Ok(()),
    }
  }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct TestRunMeta {
  pub dashboard_url: String,
}

pub enum IPCMessage<'a> {
  Log(&'a LogSchema),
  TestRunMeta(&'a TestRunMeta),
  UpdateTestCase(&'a UpdateTestCase),
}

impl<'a> IPCMessage<'a> {
  fn name(&self) -> &str {
    match self {
      IPCMessage::Log(_) => "log",
      IPCMessage::TestRunMeta(_) => "test_url",
      IPCMessage::UpdateTestCase(_) => "update_test_case",
    }
  }

  fn data(&self) -> Value {
    match self {
      IPCMessage::Log(data) => json!(data),
      IPCMessage::TestRunMeta(data) => json!(data),
      IPCMessage::UpdateTestCase(data) => json!(data),
    }
  }
}
