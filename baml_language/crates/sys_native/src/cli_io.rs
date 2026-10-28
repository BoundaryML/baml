//! Portable filesystem/archive/TOML operations used by standalone BAML CLIs.
use std::sync::Arc;

use bex_heap::BexHeap;
use sys_ops::io::{self, CallId, SysOpContext, SysOpOutput, VmBamlError, owned};

use crate::NativeSysOps;

pub(crate) fn io_error(error: impl std::fmt::Display) -> VmBamlError {
    VmBamlError::Io {
        message: error.to_string(),
    }
}

fn invalid_argument(error: impl std::fmt::Display) -> VmBamlError {
    VmBamlError::InvalidArgument {
        message: error.to_string(),
    }
}

impl io::IoNamespaceArchive for NativeSysOps {
    fn entries(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        data: Vec<u8>,
        format: String,
        _: &SysOpContext,
    ) -> SysOpOutput<Vec<String>> {
        SysOpOutput::async_op(async move {
            let entries = tokio::task::spawn_blocking(move || {
                let url = archive_name(&format)?;
                baml_release::archive_paths(&data, url).map_err(io_error)
            })
            .await
            .map_err(io_error)??;
            Ok(entries)
        })
    }

    fn extract(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        data: Vec<u8>,
        format: String,
        destination: String,
        _: &SysOpContext,
    ) -> SysOpOutput<()> {
        let destination = self.working_dir.resolve(&destination);
        SysOpOutput::async_op(async move {
            tokio::task::spawn_blocking(move || {
                let url = archive_name(&format)?;
                baml_release::archive_paths(&data, url).map_err(io_error)?;
                baml_release::extract_archive_to_dir(&data, url, &destination).map_err(io_error)
            })
            .await
            .map_err(io_error)??;
            Ok(())
        })
    }
}

fn archive_name(format: &str) -> Result<&'static str, VmBamlError> {
    match format {
        "zip" => Ok("archive.zip"),
        "tar.gz" => Ok("archive.tar.gz"),
        _ => Err(io_error(format!("unsupported archive format {format:?}"))),
    }
}

impl io::IoNamespaceToml for NativeSysOps {
    fn edit(
        &self,
        _: &Arc<BexHeap>,
        _: CallId,
        text: String,
        path: Vec<String>,
        value: Option<String>,
        remove: Vec<String>,
        _: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            edit_toml(&text, &path, value.as_deref(), &remove).map_err(Into::into)
        })
    }
}

fn edit_toml(
    text: &str,
    path: &[String],
    value: Option<&str>,
    remove: &[String],
) -> Result<String, VmBamlError> {
    use toml_edit::{DocumentMut, Item, Key, Table};
    let (key, parents) = path
        .split_last()
        .ok_or_else(|| invalid_argument("empty TOML key path"))?;
    let mut document: DocumentMut = text.parse().map_err(invalid_argument)?;
    let value = value
        .map(|value| {
            let mut parsed = format!("value = {value}")
                .parse::<DocumentMut>()
                .map_err(invalid_argument)?;
            parsed
                .remove("value")
                .ok_or_else(|| invalid_argument("expected a TOML value expression"))
        })
        .transpose()?;
    let mut item = document.as_item_mut();
    for parent in parents {
        let table = item
            .as_table_like_mut()
            .ok_or_else(|| invalid_argument("TOML path traverses a non-table"))?;
        if table.get(parent).is_none() {
            if value.is_none() {
                return Ok(text.to_owned());
            }
            table.insert(parent, Item::Table(Table::new()));
        }
        item = table.get_mut(parent).unwrap();
    }
    let table = item
        .as_table_like_mut()
        .ok_or_else(|| invalid_argument("TOML key parent is not a table"))?;
    let mut decoration = None;
    for old_key in remove
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(key.as_str()))
    {
        let old_name = table.key(old_key).cloned();
        if let Some(old_item) = table.remove(old_key) {
            if decoration.is_none() {
                decoration = old_name.map(|name| (name, old_item));
            }
        }
    }
    if let Some(mut value) = value {
        let mut name = Key::new(key);
        if let Some((old_name, old_item)) = decoration {
            *name.leaf_decor_mut() = old_name.leaf_decor().clone();
            if let (Some(old_value), Some(new_value)) = (old_item.as_value(), value.as_value_mut())
            {
                *new_value.decor_mut() = old_value.decor().clone();
            }
        }
        table.entry_format(&name).or_insert(value);
    }
    Ok(document.to_string())
}

/// An explicit signal route owned by a standalone CLI host. Ordinary embedded
/// engines leave this unset and never change the process's signal dispositions.
#[derive(Clone, Debug)]
pub struct ForegroundSignals(tokio::sync::broadcast::Sender<i32>);

impl Default for ForegroundSignals {
    fn default() -> Self {
        Self(tokio::sync::broadcast::channel(16).0)
    }
}

impl ForegroundSignals {
    /// Forward a host signal to active foreground operations. Returns false
    /// when there is no foreground child and the host should handle it itself.
    pub fn forward(&self, signal: i32) -> bool {
        self.0.send(signal).is_ok()
    }
}

pub(crate) async fn foreground(
    mut command: tokio::process::Command,
    signals: Option<ForegroundSignals>,
) -> Result<owned::sys::ProcessExit, VmBamlError> {
    use std::process::Stdio;
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let signals = signals.map(|signals| signals.0.subscribe());
    let mut child = command.spawn().map_err(io_error)?;
    let status = if let Some(mut signals) = signals {
        loop {
            let signal = tokio::select! {
                status = child.wait() => break status.map_err(io_error)?,
                signal = signals.recv() => match signal {
                    Ok(signal) => signal,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break child.wait().await.map_err(io_error)?;
                    }
                },
            };
            #[cfg(unix)]
            if let Some(pid) = child.id().and_then(|pid| i32::try_from(pid).ok()) {
                if signal == libc::SIGINT && !should_forward_interrupt(pid.cast_unsigned()) {
                    continue;
                }
                // SAFETY: pid came from this live Child; kill takes no pointers.
                #[allow(unsafe_code, reason = "forward a host-owned signal to its live child")]
                unsafe {
                    libc::kill(pid, signal);
                }
            }
            // Windows delivers console signals to the inherited child console.
            #[cfg(not(unix))]
            let _ = signal;
        }
    } else {
        child.wait().await.map_err(io_error)?
    };
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    };
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    Ok(owned::sys::ProcessExit {
        exit_code: i64::from(
            status
                .code()
                .unwrap_or_else(|| signal.map_or(1, |signal| 128 + signal)),
        ),
        signal: signal.map(|signal| signal.to_string()),
    })
}

#[cfg(unix)]
fn in_terminal_foreground_group() -> bool {
    use std::os::fd::AsRawFd;
    // A piped stdin does not remove the controlling terminal. Inspect all
    // standard descriptors and /dev/tty before deciding to forward SIGINT.
    let tty = std::fs::File::open("/dev/tty").ok();
    [
        Some(0),
        Some(1),
        Some(2),
        tty.as_ref().map(AsRawFd::as_raw_fd),
    ]
    .into_iter()
    .flatten()
    .any(|fd| {
        // SAFETY: these syscalls accept descriptor integers and no pointers.
        #[allow(unsafe_code, reason = "inspect the terminal foreground process group")]
        unsafe {
            libc::tcgetpgrp(fd) == libc::getpgrp()
        }
    })
}

#[cfg(unix)]
fn should_forward_interrupt(pid: u32) -> bool {
    if !in_terminal_foreground_group() {
        return true;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // A child that changes its process group no longer receives the terminal's
    // interrupt. Check at signal time, as uv does, rather than only at spawn.
    // SAFETY: these process queries take integer identifiers and no pointers.
    #[allow(unsafe_code, reason = "inspect the live child's process group")]
    unsafe {
        libc::getpgid(pid) != libc::getpgrp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_a_missing_toml_key_preserves_the_document() {
        for document in ["", "# preserved\n[other]\nvalue = 1\n"] {
            assert_eq!(
                edit_toml(document, &["absent".into(), "key".into()], None, &[]).unwrap(),
                document
            );
        }
    }

    #[cfg(unix)]
    fn signal_handler(signal: i32) -> libc::sighandler_t {
        let mut action = std::mem::MaybeUninit::<libc::sigaction>::uninit();
        // SAFETY: sigaction with a null action only reads the installed policy
        // into the valid output storage. The successful call initializes it.
        #[allow(unsafe_code, reason = "observe signal policy without changing it")]
        unsafe {
            assert_eq!(
                libc::sigaction(signal, std::ptr::null(), action.as_mut_ptr()),
                0
            );
            action.assume_init().sa_sigaction
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ordinary_foreground_calls_preserve_host_signal_handlers() {
        let before = [signal_handler(libc::SIGINT), signal_handler(libc::SIGTERM)];
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "exit 0"]);
        assert_eq!(foreground(command, None).await.unwrap().exit_code, 0);
        let missing = tokio::process::Command::new("/nonexistent/baml-foreground-test");
        assert!(foreground(missing, None).await.is_err());
        assert_eq!(
            before,
            [signal_handler(libc::SIGINT), signal_handler(libc::SIGTERM)]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn host_owned_signals_reach_the_foreground_child() {
        let signals = ForegroundSignals::default();
        let mut command = tokio::process::Command::new("/bin/sleep");
        command.arg("30");
        let task = tokio::spawn(foreground(command, Some(signals.clone())));
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !signals.forward(libc::SIGTERM) {
                tokio::task::yield_now().await;
            }
            task.await.unwrap().unwrap()
        })
        .await
        .unwrap();
        assert_eq!(result.exit_code, 128 + i64::from(libc::SIGTERM));
        assert_eq!(result.signal.as_deref(), Some("15"));
        assert!(!signals.forward(libc::SIGTERM));
    }
}
