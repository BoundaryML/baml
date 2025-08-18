use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::SystemTime,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct StdinRecord {
    timestamp: String,
    data: String,
}

pub fn record(file_path: &str, command: &[String]) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("Command cannot be empty");
    }

    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("Failed to spawn subprocess")?;

    let stdin = child
        .stdin
        .take()
        .context("Failed to get subprocess stdin")?;

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(file_path)
        .context("Failed to open output file")?;

    handle_stdin_record(stdin, file)?;

    let exit_status = child.wait().context("Failed to wait for subprocess")?;

    // stdin_thread
    //     .join()
    //     .map_err(|_| anyhow::anyhow!("Stdin thread panicked"))??;

    if !exit_status.success() {
        anyhow::bail!("Subprocess failed with exit code: {:?}", exit_status.code());
    }

    Ok(())
}

fn handle_stdin_record(mut stdin: std::process::ChildStdin, mut file: File) -> Result<()> {
    let parent_stdin = std::io::stdin();
    let reader = BufReader::new(parent_stdin);

    for line in reader.lines() {
        let line = line.context("Failed to read line from stdin")?;
        let line = format!("{line}\n");

        let record = StdinRecord {
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .to_string(),
            data: line.clone(),
        };

        let json_line = serde_json::to_string(&record).context("Failed to serialize record")?;
        writeln!(file, "{json_line}").context("Failed to write to file")?;
        file.flush().context("Failed to flush file")?;

        stdin
            .write_all(line.as_bytes())
            .context("Failed to write to subprocess stdin")?;
        stdin.flush().context("Failed to flush subprocess stdin")?;
    }

    Ok(())
}

pub fn replay(file_path: &str, command: &[String]) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("Command cannot be empty");
    }

    let records = load_records(file_path)?;

    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("Failed to spawn subprocess")?;

    let stdin = child
        .stdin
        .take()
        .context("Failed to get subprocess stdin")?;

    let stdin_thread = thread::spawn(move || handle_stdin_replay(stdin, records));
    let exit_status = child.wait().context("Failed to wait for subprocess")?;

    stdin_thread
        .join()
        .map_err(|_| anyhow::anyhow!("Stdin thread panicked"))??;

    if !exit_status.success() {
        anyhow::bail!("Subprocess failed with exit code: {:?}", exit_status.code());
    }

    Ok(())
}

fn load_records(file_path: &str) -> Result<Vec<StdinRecord>> {
    let file = File::open(file_path).context("Failed to open input file")?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for line in reader.lines() {
        let line = line.context("Failed to read line from file")?;
        let record: StdinRecord = StdinRecord {
            timestamp: "1970-00-00".to_string(),
            data: line,
        };
        records.push(record);
    }

    Ok(records)
}

fn handle_stdin_replay(
    mut stdin: std::process::ChildStdin,
    records: Vec<StdinRecord>,
) -> Result<()> {
    let n_records = records.len();
    for record in records {
        stdin
            .write_all(record.data.as_bytes())
            .context("Failed to write replayed data to subprocess stdin")?;
        stdin.flush().context("Failed to flush subprocess stdin")?;
    }
    eprintln!("Flushed {} records to child stdin", n_records);

    let parent_stdin = std::io::stdin();
    let reader = BufReader::new(parent_stdin);

    for line in reader.lines() {
        let line = line.context("Failed to read line from stdin")?;
        let line_with_newline = format!("{}\n", line);

        stdin
            .write_all(line_with_newline.as_bytes())
            .context("Failed to write to subprocess stdin")?;
        stdin.flush().context("Failed to flush subprocess stdin")?;
    }

    Ok(())
}
