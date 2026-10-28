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
            let url = archive_name(&format)?;
            baml_release::archive_paths(&data, url).map_err(|error| io_error(error).into())
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
            let url = archive_name(&format)?;
            baml_release::archive_paths(&data, url).map_err(io_error)?;
            baml_release::extract_archive_to_dir(&data, url, &destination)
                .map_err(|error| io_error(error).into())
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

pub(crate) async fn foreground(
    mut command: tokio::process::Command,
) -> Result<owned::sys::ProcessExit, VmBamlError> {
    use std::process::Stdio;
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    #[cfg(unix)]
    let (mut interrupt, mut terminate) = {
        use tokio::signal::unix::{SignalKind, signal};
        (
            signal(SignalKind::interrupt()).map_err(io_error)?,
            signal(SignalKind::terminate()).map_err(io_error)?,
        )
    };
    let mut child = command.spawn().map_err(io_error)?;
    #[cfg(unix)]
    let status = {
        loop {
            let signal = tokio::select! {
                status = child.wait() => break status.map_err(io_error)?,
                _ = interrupt.recv() => {
                    if child.id().is_some_and(should_forward_interrupt) {
                        libc::SIGINT
                    } else {
                        continue;
                    }
                },
                _ = terminate.recv() => libc::SIGTERM,
            };
            if let Some(pid) = child.id().and_then(|pid| i32::try_from(pid).ok()) {
                // SAFETY: pid came from this live Child; kill takes no pointers.
                #[allow(unsafe_code, reason = "forward a signal to the live foreground child")]
                unsafe {
                    libc::kill(pid, signal);
                }
            }
        }
    };
    #[cfg(windows)]
    let status = {
        let mut interrupt = tokio::signal::windows::ctrl_c().map_err(io_error)?;
        let mut quit = tokio::signal::windows::ctrl_break().map_err(io_error)?;
        loop {
            tokio::select! {
                status = child.wait() => break status.map_err(io_error)?,
                _ = interrupt.recv() => {},
                _ = quit.recv() => {},
            }
        }
    };
    #[cfg(not(any(unix, windows)))]
    let status = child.wait().await.map_err(io_error)?;
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
