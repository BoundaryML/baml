use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::{ChildStdin, ChildStdout};

pub struct LspSubprocess {
    command_args: Vec<String>,
}

impl LspSubprocess {
    pub fn new(command_args: Vec<String>) -> Result<Self> {
        if command_args.is_empty() {
            anyhow::bail!("No LSP command provided");
        }
        Ok(Self { command_args })
    }
    
    pub async fn spawn(&self) -> Result<LspProcess> {
        let mut cmd = tokio::process::Command::new(&self.command_args[0]);
        
        if self.command_args.len() > 1 {
            cmd.args(&self.command_args[1..]);
        }
        
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn LSP command: {:?}", self.command_args))?;
        
        let stdin = child.stdin.take()
            .context("Failed to get stdin handle")?;
        let stdout = child.stdout.take()
            .context("Failed to get stdout handle")?;
        
        tracing::info!("Spawned LSP process: {:?}", self.command_args);
        
        Ok(LspProcess {
            child,
            stdin,
            stdout,
        })
    }
    
    pub fn command_args(&self) -> &[String] {
        &self.command_args
    }
}

pub struct LspProcess {
    pub child: tokio::process::Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

impl LspProcess {
    pub async fn wait_for_exit(&mut self) -> Result<()> {
        let status = self.child.wait().await?;
        if !status.success() {
            anyhow::bail!("LSP process exited with status: {}", status);
        }
        Ok(())
    }
}