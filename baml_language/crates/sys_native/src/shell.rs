//! Cross-platform shell detection and invocation.
//!
//! Follows the same approach as Codex:
//! - macOS: user default → zsh → bash → /bin/sh
//! - Linux: user default → bash → zsh → /bin/sh
//! - Windows: pwsh → powershell → cmd.exe

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellType {
    Bash,
    Zsh,
    Sh,
    PowerShell,
    Cmd,
}

#[derive(Debug, Clone)]
pub struct ResolvedShell {
    pub path: PathBuf,
    pub shell_type: ShellType,
}

impl ShellType {
    /// Detect shell type from a binary path by matching the file stem.
    fn from_path(path: &Path) -> Option<ShellType> {
        let stem = path.file_stem()?.to_str()?;
        match stem {
            "zsh" => Some(ShellType::Zsh),
            "bash" => Some(ShellType::Bash),
            "sh" => Some(ShellType::Sh),
            "pwsh" | "powershell" => Some(ShellType::PowerShell),
            "cmd" => Some(ShellType::Cmd),
            _ => None,
        }
    }
}

impl ResolvedShell {
    /// Configure a `Command` to execute `command_str` through this shell.
    pub fn apply(&self, cmd: &mut tokio::process::Command, command_str: &str) {
        match self.shell_type {
            ShellType::Bash | ShellType::Zsh | ShellType::Sh => {
                cmd.arg("-c").arg(command_str);
            }
            ShellType::PowerShell => {
                cmd.args(["-NoProfile", "-Command", command_str]);
            }
            ShellType::Cmd => {
                cmd.args(["/c", command_str]);
            }
        }
    }
}

/// Returns the cached default shell for this platform.
pub fn default_shell() -> &'static ResolvedShell {
    static SHELL: OnceLock<ResolvedShell> = OnceLock::new();
    SHELL.get_or_init(detect_shell)
}

fn detect_shell() -> ResolvedShell {
    #[cfg(target_os = "windows")]
    {
        detect_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        detect_unix()
    }
}

// ---------------------------------------------------------------------------
// Unix
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
fn detect_unix() -> ResolvedShell {
    // 1. User's default shell from passwd
    if let Some(shell) = user_default_shell() {
        return shell;
    }

    // 2. Platform-ordered fallback chain
    #[cfg(target_os = "macos")]
    let candidates: &[(&str, ShellType)] =
        &[("/bin/zsh", ShellType::Zsh), ("/bin/bash", ShellType::Bash)];

    #[cfg(not(target_os = "macos"))]
    let candidates: &[(&str, ShellType)] =
        &[("/bin/bash", ShellType::Bash), ("/bin/zsh", ShellType::Zsh)];

    for (path, shell_type) in candidates {
        if Path::new(path).exists() {
            return ResolvedShell {
                path: PathBuf::from(path),
                shell_type: *shell_type,
            };
        }
    }

    // 3. Ultimate fallback
    ResolvedShell {
        path: PathBuf::from("/bin/sh"),
        shell_type: ShellType::Sh,
    }
}

/// Read the user's default shell from the SHELL environment variable.
#[cfg(not(target_os = "windows"))]
fn user_default_shell() -> Option<ResolvedShell> {
    let shell_str = std::env::var("SHELL").ok()?;
    let shell_path = PathBuf::from(&shell_str);

    if !shell_path.exists() {
        return None;
    }

    let shell_type = ShellType::from_path(&shell_path)?;
    Some(ResolvedShell {
        path: shell_path,
        shell_type,
    })
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn detect_windows() -> ResolvedShell {
    let candidates: &[(&str, ShellType)] = &[
        (
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            ShellType::PowerShell,
        ),
        (
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            ShellType::PowerShell,
        ),
    ];

    for (path, shell_type) in candidates {
        if Path::new(path).exists() {
            return ResolvedShell {
                path: PathBuf::from(path),
                shell_type: *shell_type,
            };
        }
    }

    // Ultimate fallback
    ResolvedShell {
        path: PathBuf::from("cmd.exe"),
        shell_type: ShellType::Cmd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_type_from_path() {
        assert_eq!(
            ShellType::from_path(Path::new("/bin/bash")),
            Some(ShellType::Bash)
        );
        assert_eq!(
            ShellType::from_path(Path::new("/usr/local/bin/zsh")),
            Some(ShellType::Zsh)
        );
        assert_eq!(
            ShellType::from_path(Path::new("/bin/sh")),
            Some(ShellType::Sh)
        );
        // Windows-style paths only parse correctly on Windows
        #[cfg(target_os = "windows")]
        {
            assert_eq!(
                ShellType::from_path(Path::new(r"C:\Program Files\PowerShell\7\pwsh.exe")),
                Some(ShellType::PowerShell)
            );
            assert_eq!(
                ShellType::from_path(Path::new("cmd.exe")),
                Some(ShellType::Cmd)
            );
        }
        assert_eq!(
            ShellType::from_path(Path::new("pwsh")),
            Some(ShellType::PowerShell)
        );
        assert_eq!(ShellType::from_path(Path::new("cmd")), Some(ShellType::Cmd));
        assert_eq!(ShellType::from_path(Path::new("/bin/fish")), None);
    }

    #[test]
    fn default_shell_resolves() {
        let shell = default_shell();
        // Should always resolve to something
        assert!(
            !shell.path.as_os_str().is_empty(),
            "shell path should not be empty"
        );
    }
}
