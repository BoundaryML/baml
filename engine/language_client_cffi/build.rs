use std::path::Path;

mod protoc_lang_out {
    //! # API to invoke `protoc` command programmatically
    //!
    //! API to invoke `protoc` command using API (e. g. from `build.rs`).
    //!
    //! Note that to generate `rust` code from `.proto`,
    //! [protoc-rust](https://docs.rs/protoc-rust) crate can be used,
    //! which does not require `protoc-gen-rust` present in `$PATH`.

    #![deny(missing_docs)]
    #![deny(rustdoc::broken_intra_doc_links)]

    use std::{
        ffi::{OsStr, OsString},
        io,
        path::{Path, PathBuf},
        process,
    };

    /// Alias for io::Error
    pub type Error = io::Error;
    /// Alias for io::Error
    pub type Result<T> = io::Result<T>;

    fn err_other(s: impl AsRef<str>) -> Error {
        Error::other(s.as_ref().to_owned())
    }

    /// `protoc --lang_out=... ...` command builder and spawner.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use protoc::ProtocLangOut;
    /// ProtocLangOut::new()
    ///     .lang("go")
    ///     .include("protos")
    ///     .include("more-protos")
    ///     .out_dir("generated-protos")
    ///     .run()
    ///     .unwrap();
    /// ```
    #[derive(Default)]
    pub struct ProtocLangOut {
        protoc: Option<Protoc>,
        /// `LANG` part in `--LANG_out=...`
        lang: Option<String>,
        /// `--LANG_out=...` param
        out_dir: Option<PathBuf>,
        /// `--plugin` param. Not needed if plugin is in `$PATH`
        plugin: Option<OsString>,
        /// `-I` args
        includes: Vec<PathBuf>,
        /// List of `.proto` files to compile
        inputs: Vec<PathBuf>,
    }

    impl ProtocLangOut {
        /// Arguments to the `protoc` found in `$PATH`
        pub fn new() -> Self {
            Self::default()
        }

        /// Set `LANG` part in `--LANG_out=...`
        pub fn lang(&mut self, lang: &str) -> &mut Self {
            self.lang = Some(lang.to_owned());
            self
        }

        /// Set `--LANG_out=...` param
        pub fn out_dir(&mut self, out_dir: impl AsRef<Path>) -> &mut Self {
            self.out_dir = Some(out_dir.as_ref().to_owned());
            self
        }

        /// Set `--plugin` param. Not needed if plugin is in `$PATH`
        pub fn plugin(&mut self, plugin: impl AsRef<OsStr>) -> &mut Self {
            self.plugin = Some(plugin.as_ref().to_owned());
            self
        }

        /// Append a path to `-I` args
        pub fn include(&mut self, include: impl AsRef<Path>) -> &mut Self {
            self.includes.push(include.as_ref().to_owned());
            self
        }

        /// Append multiple paths to `-I` args
        pub fn includes(
            &mut self,
            includes: impl IntoIterator<Item = impl AsRef<Path>>,
        ) -> &mut Self {
            for include in includes {
                self.include(include);
            }
            self
        }

        /// Append a `.proto` file path to compile
        pub fn input(&mut self, input: impl AsRef<Path>) -> &mut Self {
            self.inputs.push(input.as_ref().to_owned());
            self
        }

        /// Append multiple `.proto` file paths to compile
        pub fn inputs(&mut self, inputs: impl IntoIterator<Item = impl AsRef<Path>>) -> &mut Self {
            for input in inputs {
                self.input(input);
            }
            self
        }

        /// Execute `protoc` with given args
        pub fn run(&self) -> Result<()> {
            let protoc = match &self.protoc {
                Some(protoc) => protoc.clone(),
                None => {
                    let protoc = Protoc::from_vendored();
                    // Check with have good `protoc`
                    protoc.check()?;
                    protoc
                }
            };

            if self.inputs.is_empty() {
                return Err(err_other("input is empty"));
            }

            let out_dir = self
                .out_dir
                .as_ref()
                .ok_or_else(|| err_other("out_dir is empty"))?;
            let lang = self
                .lang
                .as_ref()
                .ok_or_else(|| err_other("lang is empty"))?;

            // --{lang}_out={out_dir}
            let mut lang_out_flag = OsString::from("--");
            lang_out_flag.push(lang);
            lang_out_flag.push("_out=");
            lang_out_flag.push(out_dir);

            // --plugin={plugin}
            let plugin_flag = self.plugin.as_ref().map(|plugin| {
                let mut flag = OsString::from("--plugin=");
                flag.push(plugin);
                flag
            });

            // -I{include}
            let include_flags = self.includes.iter().map(|include| {
                let mut flag = OsString::from("-I");
                flag.push(include);
                flag
            });

            let mut cmd_args = Vec::new();
            cmd_args.push(lang_out_flag);
            cmd_args.extend(self.inputs.iter().map(|path| path.as_os_str().to_owned()));
            cmd_args.extend(plugin_flag);
            cmd_args.extend(include_flags);
            protoc.run_with_args(cmd_args)
        }
    }

    /// `Protoc --descriptor_set_out...` args
    #[derive(Debug)]
    pub struct DescriptorSetOutArgs<'a> {
        /// `--file_descriptor_out=...` param
        pub out: &'a str,
        /// `-I` args
        pub includes: &'a [&'a str],
        /// List of `.proto` files to compile
        pub input: &'a [&'a str],
        /// `--include_imports`
        pub include_imports: bool,
    }

    /// Protoc command.
    #[derive(Clone, Debug)]
    pub struct Protoc {
        exec: OsString,
    }

    impl Protoc {
        pub fn from_vendored() -> Protoc {
            Protoc {
                exec: protoc_bin_vendored::protoc_bin_path()
                    .unwrap()
                    .into_os_string(),
            }
        }

        /// New `protoc` command from specified path
        ///
        /// # Examples
        ///
        /// ```no_run
        /// # mod protoc_bin_vendored {
        /// #   pub fn protoc_bin_path() -> Result<std::path::PathBuf, std::io::Error> {
        /// #       unimplemented!()
        /// #   }
        /// # }
        ///
        /// // Use a binary from `protoc-bin-vendored` crate
        /// let protoc = protoc::Protoc::from_path(
        ///     protoc_bin_vendored::protoc_bin_path().unwrap());
        /// ```
        pub fn from_path(path: impl AsRef<OsStr>) -> Protoc {
            Protoc {
                exec: path.as_ref().to_owned(),
            }
        }

        /// Check `protoc` command found and valid
        pub fn check(&self) -> Result<()> {
            self.version().map(|_| ())
        }

        fn spawn(&self, cmd: &mut process::Command) -> io::Result<process::Child> {
            println!("spawning command {cmd:?}");
            cmd.spawn()
                .map_err(|e| Error::new(e.kind(), format!("failed to spawn `{cmd:?}`: {e}")))
        }

        /// Obtain `protoc` version
        pub fn version(&self) -> Result<Version> {
            let child = self.spawn(
                process::Command::new(&self.exec)
                    .stdin(process::Stdio::null())
                    .stdout(process::Stdio::piped())
                    .stderr(process::Stdio::piped())
                    .args(["--version"]),
            )?;

            let output = child.wait_with_output()?;
            if !output.status.success() {
                return Err(err_other("protoc failed with error"));
            }
            let output = String::from_utf8(output.stdout).map_err(Error::other)?;
            let output = match output.lines().next() {
                None => return Err(err_other("output is empty")),
                Some(line) => line,
            };
            let prefix = "libprotoc ";
            if !output.starts_with(prefix) {
                return Err(err_other("output does not start with prefix"));
            }
            let output = &output[prefix.len()..];
            if output.is_empty() {
                return Err(err_other("version is empty"));
            }
            let first = output.chars().next().unwrap();
            if !first.is_ascii_digit() {
                return Err(err_other("version does not start with digit"));
            }
            Ok(Version {
                version: output.to_owned(),
            })
        }

        /// Execute `protoc` command with given args, check it completed correctly.
        fn run_with_args(&self, args: Vec<OsString>) -> Result<()> {
            let mut cmd = process::Command::new(&self.exec);
            cmd.stdin(process::Stdio::null());
            cmd.args(args);

            let mut child = self.spawn(&mut cmd)?;

            if !child.wait()?.success() {
                return Err(err_other(format!(
                    "protoc ({cmd:?}) exited with non-zero exit code"
                )));
            }

            Ok(())
        }

        /// Execute `protoc --descriptor_set_out=`
        pub fn write_descriptor_set(&self, args: DescriptorSetOutArgs) -> Result<()> {
            let mut cmd_args: Vec<OsString> = Vec::new();

            for include in args.includes {
                cmd_args.push(format!("-I{include}").into());
            }

            if args.out.is_empty() {
                return Err(err_other("out is empty"));
            }

            cmd_args.push(format!("--descriptor_set_out={}", args.out).into());

            if args.include_imports {
                cmd_args.push("--include_imports".into());
            }

            if args.input.is_empty() {
                return Err(err_other("input is empty"));
            }

            cmd_args.extend(args.input.iter().map(|a| OsString::from(*a)));

            self.run_with_args(cmd_args)
        }
    }

    pub struct Version {
        pub version: String,
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn version() {
            Protoc::from_env_path().version().expect("version");
        }
    }
}

fn main() -> std::io::Result<()> {
    println!("running build for baml_cffi");
    // Re-run build.rs if these files change.
    println!("cargo:rerun-if-changed=types/cffi.proto");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/ctypes/baml_type_encode.rs");
    println!("cargo:rerun-if-changed=src/ctypes/baml_value_encode.rs");
    println!("cargo:rerun-if-changed=src/ctypes/baml_type_decode.rs");
    println!("cargo:rerun-if-changed=build.rs");

    std::env::set_var(
        "PROTOC",
        protoc_bin_vendored::protoc_bin_path()
            .unwrap()
            .to_str()
            .unwrap(),
    );
    prost_build::compile_protos(&["types/cffi.proto"], &["types/"])?;

    {
        let lang = "go";
        let lang_dir = format!("../language_client_{lang}/pkg");
        // let args: flatc_rust::Args<'_> = flatc_rust::Args {
        //     lang,
        //     inputs: &[Path::new("types/cffi.fbs")],
        //     out_dir: Path::new(&lang_dir),
        //     ..Default::default()
        // };

        let mut protoc = protoc_lang_out::ProtocLangOut::new();
        protoc
            .lang(lang)
            .input("types/cffi.proto")
            .out_dir(lang_dir);

        // Allow overriding the protoc-gen-go plugin path
        if let Ok(path) = std::env::var("PROTOC_GEN_GO_PATH") {
            protoc.plugin(&path);
        } else {
            // Try to find protoc-gen-go using mise
            match std::process::Command::new("mise")
                .args(["which", "protoc-gen-go"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let path = String::from_utf8_lossy(&output.stdout);
                    let path = path.trim();
                    eprintln!("Using protoc-gen-go from mise: {path:?}");
                    protoc.plugin(path);
                }
                Ok(_) => {
                    eprintln!(
                        "protoc-gen-go fallback: mise which protoc-gen-go failed, relying on PATH"
                    );
                }
                Err(e) => {
                    eprintln!("protoc-gen-go fallback: mise command failed ({e}), relying on PATH");
                }
            }
        }

        protoc
            .run()
            .unwrap_or_else(|_| panic!("Failed to generate {lang} bindings"));
    }

    // Use cbindgen to generate the C header for your Rust library.
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    {
        // Generate header to pkg/cffi directory for better vendoring support
        let out_path =
            Path::new(&crate_dir).join("../language_client_go/pkg/cffi/baml_cffi_generated.h");
        let outpath_content =
            std::fs::read_to_string(&out_path).unwrap_or_else(|_| String::from(""));
        let res = cbindgen::Builder::new()
            .with_config(cbindgen::Config::from_file("cbindgen.toml").unwrap())
            .with_crate(".")
            .generate()
            .expect("Failed to generate C header")
            .write_to_file(out_path.clone());
        if std::env::var("CI").is_ok() && res {
            let new_content = std::fs::read_to_string(&out_path).unwrap();
            println!("New header content: \n==============\n{new_content}");
            println!("\n\n");
            println!("Old header content: \n==============\n{outpath_content}");
            panic!("cbindgen generated a diff");
        }
    }

    Ok(())
}
