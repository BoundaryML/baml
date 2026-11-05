//! `package_name` in `[generator.<name>]`: the name of the generated SDK, in
//! the target language's terms. It also picks the directory under
//! `output_dir` that the SDK occupies.
//!
//! A configurable directory makes the output writer's ownership semantics
//! riskier than with the fixed `baml_sdk`: the writer overwrites colliding
//! files and git-ignores its whole directory, so a mistyped name could land on
//! hand-written code. An explicitly named SDK therefore only installs into a
//! directory that is missing, empty, or already owned by `baml generate`.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use baml_sdkgen_types::{
    DEFAULT_PACKAGE_NAME, GENERATED_GITIGNORE, OutputType, is_generated_output_directory,
    is_safe_portable_segment,
};

/// Why a target refuses a `package_name`.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PackageNameError {
    /// The target cannot rename its generated SDK yet.
    Unsupported,
    /// The value is not a name the target accepts; the payload says what is.
    Invalid(&'static str),
}

/// Check `name` against `output_type`'s naming rules.
pub(super) fn validate(output_type: OutputType, name: &str) -> Result<(), PackageNameError> {
    match output_type {
        OutputType::PythonPydantic | OutputType::PythonPydanticV1 => validate_python(name),
        OutputType::TypescriptNode
        | OutputType::TypescriptWeb
        | OutputType::Swift
        | OutputType::Go
        | OutputType::Rust
        | OutputType::Java
        | OutputType::Cpp
        | OutputType::CSharp => Err(PackageNameError::Unsupported),
    }
}

/// The SDK's directory relative to `output_dir`. A Python import path nests
/// one directory per segment, so `output_dir` is the import root.
pub(super) fn relative_directory(output_type: OutputType, package_name: Option<&str>) -> PathBuf {
    let name = package_name.unwrap_or(DEFAULT_PACKAGE_NAME);
    match output_type {
        OutputType::PythonPydantic | OutputType::PythonPydanticV1 => name.split('.').collect(),
        _ => PathBuf::from(name),
    }
}

/// Entries that tools and operating systems leave behind; they hold no
/// hand-written code, so they do not make a directory someone else's.
const DISPOSABLE_ENTRIES: &[&str] = &["__pycache__", ".DS_Store", "Thumbs.db"];

/// Refuse to install `generator`'s SDK into `directory` when it holds files
/// that `baml generate` did not write there.
pub(super) fn ensure_claimable(generator: &str, directory: &Path) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", directory.display()));
        }
    };
    let mut foreign = None;
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read {}", directory.display()))?;
        let name = entry.file_name();
        if !DISPOSABLE_ENTRIES
            .iter()
            .any(|disposable| name == *disposable)
            && !is_generated_gitignore(&name, &entry.path())
        {
            foreign = Some(name);
            break;
        }
    }
    let Some(foreign) = foreign else {
        return Ok(());
    };
    if is_generated_output_directory(directory)? {
        return Ok(());
    }
    anyhow::bail!(
        "refusing to generate `{generator}` into {}: it already contains `{}`, and `baml \
         generate` did not create the directory. Generating would overwrite files there and \
         git-ignore the whole directory. Set `package_name` to a name that does not collide with \
         your own code, or empty the directory first.",
        directory.display(),
        foreign.to_string_lossy()
    )
}

/// The `.gitignore` `baml generate` writes into every SDK directory. It
/// un-ignores itself so it can be committed, so a fresh clone of a project
/// that commits it holds that file and nothing else.
fn is_generated_gitignore(name: &std::ffi::OsStr, path: &Path) -> bool {
    name == ".gitignore"
        && path.is_file()
        && fs::read(path).is_ok_and(|contents| contents == GENERATED_GITIGNORE.as_bytes())
}

/// Output an earlier `baml generate` left at the default location, now that
/// `package_name` installs the SDK elsewhere. `destinations` are the
/// directories this run writes: output a generator still writes is not
/// superseded.
pub(super) fn superseded_default_output(
    output_root: &Path,
    destinations: &[PathBuf],
) -> Option<PathBuf> {
    let previous = output_root.join(DEFAULT_PACKAGE_NAME);
    // Compare resolved paths too, so `../project/out` and `out` are the same
    // destination. A destination that doesn't exist yet stays as written.
    let resolved = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let resolved_previous = resolved(&previous);
    (!destinations.iter().any(|destination| {
        destination.starts_with(&previous) || resolved(destination).starts_with(&resolved_previous)
    }) && is_generated_output_directory(&previous).unwrap_or(false))
    .then_some(previous)
}

fn validate_python(name: &str) -> Result<(), PackageNameError> {
    if !name.split('.').all(|segment| {
        is_ascii_identifier(segment)
            && !PYTHON_KEYWORDS.contains(&segment)
            && is_safe_portable_segment(segment)
    }) {
        return Err(PackageNameError::Invalid(
            "expected a dotted Python import path such as `my_app._generated`; each segment must \
             be an ASCII identifier that is not a Python keyword or a reserved Windows file name",
        ));
    }
    if name
        .split('.')
        .next()
        .is_some_and(|top| PYTHON_RUNTIME_PACKAGES.contains(&top))
    {
        return Err(PackageNameError::Invalid(
            "the top-level package would shadow the BAML Python runtime or one of its \
             dependencies (`baml_bridge`, `google`, `pydantic`, `pydantic_core`, \
             `typing_extensions`); choose a package outside them",
        ));
    }
    if name.split('.').next().is_some_and(|top| {
        PYTHON_STDLIB_MODULES
            .split_whitespace()
            .any(|module| module == top)
    }) {
        return Err(PackageNameError::Invalid(
            "the top-level package would shadow a Python standard-library module, which the \
             generated SDK and the BAML runtime import; choose a package outside the standard \
             library",
        ));
    }
    Ok(())
}

/// Top-level packages the BAML Python runtime imports: `baml_bridge` itself
/// and its dependencies (`google` is protobuf's). A generated SDK with one of
/// these names would shadow it and fail to import.
const PYTHON_RUNTIME_PACKAGES: &[&str] = &[
    "baml_bridge",
    "google",
    "pydantic",
    "pydantic_core",
    "typing_extensions",
];

/// Python's standard-library top-level modules (`sys.stdlib_module_names`,
/// CPython 3.15, without private names). With `output_dir` first on
/// `sys.path`, a generated SDK named like one of them shadows it for the
/// SDK, the runtime and the application.
const PYTHON_STDLIB_MODULES: &str = "\
    __future__ abc antigravity argparse array ast asyncio atexit base64 bdb binascii bisect \
    builtins bz2 cProfile calendar cmath cmd code codecs codeop collections colorsys compileall \
    concurrent configparser contextlib contextvars copy copyreg csv ctypes curses dataclasses \
    datetime dbm decimal difflib dis doctest email encodings ensurepip enum errno faulthandler \
    fcntl filecmp fileinput fnmatch fractions ftplib functools gc genericpath getopt getpass \
    gettext glob graphlib grp gzip hashlib heapq hmac html http idlelib imaplib importlib \
    inspect io ipaddress itertools json keyword linecache locale logging lzma mailbox marshal \
    math mimetypes mmap modulefinder msvcrt multiprocessing netrc nt ntpath nturl2path numbers \
    opcode operator optparse os pathlib pdb pickle pickletools pkgutil platform plistlib poplib \
    posix posixpath pprint profile pstats pty pwd py_compile pyclbr pydoc pydoc_data pyexpat \
    queue quopri random re readline reprlib resource rlcompleter runpy sched secrets select \
    selectors shelve shlex shutil signal site smtplib socket socketserver sqlite3 sre_compile \
    sre_constants sre_parse ssl stat statistics string stringprep struct subprocess symtable sys \
    sysconfig syslog tabnanny tarfile tempfile termios textwrap this threading time timeit \
    tkinter token tokenize tomllib trace traceback tracemalloc tty turtle turtledemo types \
    typing unicodedata unittest urllib uuid venv warnings wave weakref webbrowser winreg \
    winsound wsgiref xml xmlrpc zipapp zipfile zipimport zlib zoneinfo";

/// `[A-Za-z_][A-Za-z0-9_]*`.
fn is_ascii_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Hard keywords; soft keywords such as `match` and `type` are valid module
/// names.
const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use baml_sdkgen_types::{
        GENERATED_GITIGNORE, GeneratedOutputFile, OutputType, write_generated_output,
    };

    use super::{
        PackageNameError, ensure_claimable, relative_directory, superseded_default_output, validate,
    };

    #[test]
    fn python_accepts_dotted_import_paths() {
        for output_type in [OutputType::PythonPydantic, OutputType::PythonPydanticV1] {
            for valid in [
                "alien_sdk",
                "alien_sdk._generated",
                "_a.B9.c_",
                "match.type",
            ] {
                assert_eq!(validate(output_type, valid), Ok(()), "{valid}");
            }
        }
    }

    #[test]
    fn python_rejects_malformed_paths_keywords_the_runtime_and_the_standard_library() {
        for invalid in [
            "",
            ".",
            "a.",
            ".a",
            "a..b",
            "9a",
            "a-b",
            "a/b",
            "a b",
            "pàckage",
            "class",
            "a.import",
            "None",
            "baml_bridge",
            "baml_bridge.generated",
            "pydantic",
            "pydantic_core.sdk",
            "typing_extensions",
            "google.sdk",
            "typing",
            "json.sdk",
            "asyncio",
            "__future__",
            "con.sdk",
            "my_app.aux",
            "my_app.COM1",
        ] {
            assert!(
                matches!(
                    validate(OutputType::PythonPydantic, invalid),
                    Err(PackageNameError::Invalid(_))
                ),
                "accepted {invalid:?}"
            );
        }
        // Only top-level packages shadow the runtime.
        for nested in [
            "my_app.baml_bridge",
            "my_app.pydantic",
            "my_app.google",
            "my_app.typing",
            "my_app.json",
        ] {
            assert_eq!(
                validate(OutputType::PythonPydantic, nested),
                Ok(()),
                "{nested}"
            );
        }
    }

    #[test]
    fn targets_without_package_name_support_reject_it() {
        for &output_type in OutputType::all() {
            if matches!(
                output_type,
                OutputType::PythonPydantic | OutputType::PythonPydanticV1
            ) {
                continue;
            }
            assert_eq!(
                validate(output_type, "baml_sdk"),
                Err(PackageNameError::Unsupported),
                "{output_type}"
            );
        }
    }

    #[test]
    fn directories_default_to_baml_sdk_and_nest_python_import_paths() {
        for &output_type in OutputType::all() {
            assert_eq!(
                relative_directory(output_type, None),
                PathBuf::from("baml_sdk")
            );
        }
        assert_eq!(
            relative_directory(OutputType::PythonPydantic, Some("alien_sdk._generated")),
            PathBuf::from("alien_sdk").join("_generated")
        );
        assert_eq!(
            relative_directory(OutputType::PythonPydanticV1, Some("pkg")),
            PathBuf::from("pkg")
        );
    }

    #[test]
    fn guard_allows_missing_empty_and_previously_generated_directories() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");
        ensure_claimable("py", &missing).unwrap();

        let empty = directory.path().join("empty");
        fs::create_dir(&empty).unwrap();
        ensure_claimable("py", &empty).unwrap();

        let leftovers = directory.path().join("leftovers");
        fs::create_dir_all(leftovers.join("__pycache__")).unwrap();
        fs::write(leftovers.join(".DS_Store"), "").unwrap();
        ensure_claimable("py", &leftovers).unwrap();

        let owned = directory.path().join("owned");
        write_generated_output(
            &owned,
            vec![GeneratedOutputFile::new("__init__.py", "x = 1\n")],
        )
        .unwrap();
        ensure_claimable("py", &owned).unwrap();
    }

    #[test]
    fn guard_allows_a_directory_holding_only_the_generated_gitignore() {
        // A fresh clone of a project that commits the SDK's `.gitignore`.
        let directory = tempfile::tempdir().unwrap();
        let cloned = directory.path().join("cloned");
        fs::create_dir(&cloned).unwrap();
        fs::write(cloned.join(".gitignore"), GENERATED_GITIGNORE).unwrap();
        ensure_claimable("py", &cloned).unwrap();
    }

    #[test]
    fn guard_refuses_a_gitignore_baml_did_not_write() {
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path().join("alien_sdk");
        fs::create_dir(&package).unwrap();
        fs::write(package.join(".gitignore"), "*.pyc\n").unwrap();

        let error = ensure_claimable("py", &package).unwrap_err().to_string();

        assert!(error.contains("already contains `.gitignore`"), "{error}");
    }

    #[test]
    fn guard_refuses_a_directory_of_hand_written_files() {
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path().join("alien_sdk");
        fs::create_dir(&package).unwrap();
        fs::write(package.join("__init__.py"), "# mine\n").unwrap();

        let error = ensure_claimable("py", &package).unwrap_err().to_string();

        assert!(error.contains("refusing to generate `py` into"), "{error}");
        assert!(error.contains(&package.display().to_string()), "{error}");
        assert!(error.contains("already contains `__init__.py`"), "{error}");
        assert!(error.contains("Set `package_name`"), "{error}");
        assert_eq!(
            fs::read_to_string(package.join("__init__.py")).unwrap(),
            "# mine\n"
        );
    }

    #[test]
    fn superseded_output_requires_an_owned_default_directory_outside_the_new_one() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let renamed = root.join("alien_sdk").join("_generated");
        assert_eq!(
            superseded_default_output(root, std::slice::from_ref(&renamed)),
            None
        );

        // Hand-written files named `baml_sdk` are not ours to point at.
        fs::create_dir(root.join("baml_sdk")).unwrap();
        fs::write(root.join("baml_sdk").join("notes.txt"), "mine").unwrap();
        assert_eq!(
            superseded_default_output(root, std::slice::from_ref(&renamed)),
            None
        );
        fs::remove_dir_all(root.join("baml_sdk")).unwrap();

        write_generated_output(
            &root.join("baml_sdk"),
            vec![GeneratedOutputFile::new("__init__.py", "x = 1\n")],
        )
        .unwrap();
        assert_eq!(
            superseded_default_output(root, std::slice::from_ref(&renamed)),
            Some(root.join("baml_sdk"))
        );
        // Never suggest deleting the directory the SDK now lives in.
        assert_eq!(
            superseded_default_output(root, &[root.join("baml_sdk")]),
            None
        );
        assert_eq!(
            superseded_default_output(root, &[root.join("baml_sdk").join("inner")]),
            None
        );
        // Nor one another generator in the same run still writes, however
        // its path is spelled.
        assert_eq!(
            superseded_default_output(root, &[renamed.clone(), root.join("baml_sdk")]),
            None
        );
        fs::create_dir(root.join("alien_sdk")).unwrap();
        let aliased = root.join("alien_sdk").join("..").join("baml_sdk");
        assert_eq!(superseded_default_output(root, &[renamed, aliased]), None);
    }
}
