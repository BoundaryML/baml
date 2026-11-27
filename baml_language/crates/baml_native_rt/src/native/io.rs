//! `baml.io`: the program's own input and output.
//!
//! The four that print write and flush where they are called, on the
//! standard library's own locked handles: nothing to park the task for,
//! and a program that only prints links no I/O runtime. A stream that is
//! closed is not an error the program can act on, so nothing is thrown.

use std::io::Write as _;

use super::{NativeBamlIo, Natives, Root, Scope, Str, StrRef, Thrown};

/// Write `text`, and then `end`, to `out` in one piece, so that two tasks
/// printing lines do not interleave within one.
fn put(mut out: impl std::io::Write, text: &str, end: &str) {
    let _ = if end.is_empty() {
        out.write_all(text.as_bytes())
    } else {
        out.write_all([text, end].concat().as_bytes())
    };
    let _ = out.flush();
}

impl NativeBamlIo for Natives {
    fn print(s: StrRef<'_>) {
        put(std::io::stdout().lock(), s.as_str(), "");
    }

    fn println(s: StrRef<'_>) {
        put(std::io::stdout().lock(), s.as_str(), "\n");
    }

    fn eprint(s: StrRef<'_>) {
        put(std::io::stderr().lock(), s.as_str(), "");
    }

    fn eprintln(s: StrRef<'_>) {
        put(std::io::stderr().lock(), s.as_str(), "\n");
    }

    async fn input<'s>(
        scope: &mut Scope<'s>,
        prompt: Option<Root<'s, Str>>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let prompt = prompt.map(|prompt| prompt.get(scope).as_str().to_owned());
        // The standard input is one buffered reader for the whole process,
        // so what one call reads past its line the next call finds.
        let read = scope
            .io(async move {
                tokio::task::spawn_blocking(move || {
                    if let Some(prompt) = prompt {
                        let mut out = std::io::stdout().lock();
                        out.write_all(prompt.as_bytes())
                            .map_err(|error| format!("Failed to write prompt: {error}"))?;
                        out.flush()
                            .map_err(|error| format!("Failed to flush stdout: {error}"))?;
                    }
                    let mut line = String::new();
                    std::io::stdin()
                        .read_line(&mut line)
                        .map_err(|error| format!("Failed to read stdin: {error}"))?;
                    if line.ends_with('\n') {
                        line.pop();
                    }
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    Ok(line)
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()))
            })
            .await;
        match read {
            Ok(line) => Ok(scope.alloc_str(&line)),
            Err(message) => Err(scope.io_error(&message)),
        }
    }
}
