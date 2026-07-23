use std::{
    collections::HashMap,
    fmt,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

#[derive(Debug)]
pub struct GoGenerationError {
    path: PathBuf,
    detail: String,
}

impl GoGenerationError {
    fn new(path: &Path, detail: impl Into<String>) -> Self {
        Self {
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for GoGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "failed to format generated Go file `{}`: {}",
            self.path.display(),
            self.detail
        )
    }
}

impl std::error::Error for GoGenerationError {}

pub(super) fn gofmt_generated_files(
    mut files: HashMap<PathBuf, String>,
) -> Result<HashMap<PathBuf, String>, GoGenerationError> {
    // HashMap iteration is intentionally randomized. Sort the paths so that a
    // malformed source tree always reports the same first formatting error.
    let mut paths = files.keys().cloned().collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        let source = files
            .get(&path)
            .expect("path collected from generated file map");
        let formatted = gofmt_source(&path, source)?;
        files.insert(path, formatted);
    }

    Ok(files)
}

fn gofmt_source(path: &Path, source: &str) -> Result<String, GoGenerationError> {
    let mut child = Command::new("gofmt")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            GoGenerationError::new(
                path,
                format!(
                    "could not start `gofmt`: {error}. Install the Go toolchain and ensure `gofmt` is on PATH"
                ),
            )
        })?;

    let mut stdin = child
        .stdin
        .take()
        .expect("gofmt stdin was configured as piped");
    let (output, write_result) = thread::scope(|scope| {
        // `gofmt` normally consumes all input before producing output, but the
        // subprocess contract does not require that behavior. Write stdin on
        // a worker while `wait_with_output` drains stdout and stderr so no
        // combination of full OS pipes can deadlock generation.
        let writer = scope.spawn(move || stdin.write_all(source.as_bytes()));
        let output = child.wait_with_output();
        let write_result = writer
            .join()
            .expect("the gofmt stdin writer does not contain panic paths");
        (output, write_result)
    });
    let output = output.map_err(|error| {
        GoGenerationError::new(path, format!("could not wait for `gofmt`: {error}"))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(GoGenerationError::new(
            path,
            if detail.is_empty() {
                format!("`gofmt` exited with status {}", output.status)
            } else {
                format!("`gofmt` exited with status {}: {detail}", output.status)
            },
        ));
    }
    write_result.map_err(|error| {
        GoGenerationError::new(path, format!("could not send source to `gofmt`: {error}"))
    })?;

    String::from_utf8(output.stdout).map_err(|error| {
        GoGenerationError::new(path, format!("`gofmt` returned non-UTF-8 output: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_generated_path_for_invalid_go() {
        if Command::new("gofmt").arg("-h").output().is_err() {
            return;
        }
        let path = Path::new("packages/example/types.go");
        let error = gofmt_source(path, "package example\nfunc {").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("packages/example/types.go"), "{message}");
        assert!(message.contains("gofmt"), "{message}");
    }

    #[test]
    fn formats_sources_far_larger_than_os_pipe_capacity() {
        const PAYLOAD_SIZE: usize = 8 * 1024 * 1024;
        if Command::new("gofmt").arg("-h").output().is_err() {
            return;
        }
        let mut source = String::with_capacity(PAYLOAD_SIZE + 40);
        source.push_str("package example\nvar Value = \"");
        source.extend(std::iter::repeat_n('z', PAYLOAD_SIZE));
        source.push_str("\"\n");

        let formatted = gofmt_source(Path::new("large.go"), &source).unwrap();

        assert!(formatted.starts_with("package example\n\nvar Value = \""));
        assert!(formatted.ends_with("\"\n"));
        assert_eq!(
            formatted.bytes().filter(|byte| *byte == b'z').count(),
            PAYLOAD_SIZE
        );
    }
}
