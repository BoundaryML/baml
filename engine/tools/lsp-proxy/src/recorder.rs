use anyhow::Result;
use lsp_server::Message;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use language_server::recording::format::{RecordedMessage, MessageDirection};

pub struct ProxyRecorder {
    writer: Arc<Mutex<BufWriter<std::fs::File>>>,
}

impl ProxyRecorder {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true) // Append instead of truncate
            .open(path)?;
        
        let writer = BufWriter::new(file);
        
        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
        })
    }
    
    pub fn record_incoming(&self, message: &Message) -> Result<()> {
        let recorded_msg = RecordedMessage {
            timestamp: SystemTime::now(),
            direction: MessageDirection::Incoming,
            message: message.clone(),
        };
        self.record_message(recorded_msg)
    }
    
    pub fn record_outgoing(&self, message: &Message) -> Result<()> {
        let recorded_msg = RecordedMessage {
            timestamp: SystemTime::now(),
            direction: MessageDirection::Outgoing,
            message: message.clone(),
        };
        self.record_message(recorded_msg)
    }
    
    fn record_message(&self, recorded_msg: RecordedMessage) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        let json_line = serde_json::to_string(&recorded_msg)?;
        writeln!(writer, "{}", json_line)?;
        writer.flush()?;
        Ok(())
    }
}

impl Drop for ProxyRecorder {
    fn drop(&mut self) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.flush();
        }
    }
}