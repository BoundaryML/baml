class WrapperUpdater {
    executable: string,

    function new() -> Self {
        if (!self_update_enabled()) {
            fail(
                `
                    self-update is disabled in this build.
                    Update BAML with your package manager.
                    Homebrew: brew upgrade baml
                    WinGet: winget upgrade --id BoundaryML.BAML
                    Arch Linux: sudo pacman -Syu baml
                    To check language toolchain updates, run: baml toolchain status
                `,
            );
        }
        WrapperUpdater { executable: baml.sys.executable() }
    }

    function update(telemetry: WrapperTelemetry? = null) -> null {
        WrapperUpdater.new().apply() catch (error) {
            _ => {
                telemetry_event(telemetry, "wrapper_self_update", fields = { "outcome": "failure" });
                throw error;
            }
        };
        telemetry_event(telemetry, "wrapper_self_update", fields = { "outcome": "success" });
        null
    }

    function apply(self) -> null {
        let release = parse_release<WrapperManifest>(fetch_text(`${ReleaseStore.new().base}/wrapper.json`));
        let artifact = release.artifacts[baml.sys.host_target()];
        let data = artifact.download();
        let format = if (artifact.url.ends_with(".zip")) {
            "zip"
        } else {
            "tar.gz"
        };
        let entries = baml.archive.entries(data, format);
        let name = if (windows()) {
            "bin/baml.exe"
        } else {
            "bin/baml"
        };
        ensure(entries.includes(name), "wrapper archive missing bin/baml");
        let token = `${baml.sys.pid()}.${int.random(0, int.max_value())}`;
        let staging = `${self.executable}.${token}.unpacked`;
        baml.fs.mkdir(staging, baml.fs.MkdirOptions { recursive: false });
        defer { baml.fs.remove_dir_all(staging); }
        baml.archive.extract(data, format, staging);
        let temp = `${self.executable}.${token}.tmp`;
        baml.fs.rename(`${staging}/${name}`, temp);
        if (windows()) {
            let helper = `${self.executable}.updater.exe`;
            let file = baml.fs.open(self.executable, "r");
            defer { file.close(); }
            baml.fs.write_bytes(helper, file.bytes());
            baml.sys.start_detached(helper, ["--replace", temp, self.executable]);
        } else {
            baml.fs.rename(temp, self.executable);
        }
        baml.io.println(`updated BAML wrapper to ${release.version}`);
        baml.io.println("To check language toolchain updates, run: baml toolchain status");
        null
    }

    function replace(args: string[]) -> null {
        ensure(args.length() == 2, "--replace requires source and destination");
        let start = baml.time.Instant.now();
        while (start.elapsed().to_milliseconds() < WRAPPER_REPLACEMENT_TIMEOUT.to_milliseconds()) {
            baml.fs.rename(args[0], args[1]) catch (error) {
                baml.errors.Io => {
                    baml.sys.sleep(WRAPPER_REPLACEMENT_RETRY_INTERVAL);
                    continue;
                },
            };
            return null;
        }
        fail(`failed to replace wrapper ${args[1]} after bounded wait`)
    }
}

const WRAPPER_REPLACEMENT_TIMEOUT = baml.time.Duration.from_seconds(5);
const WRAPPER_REPLACEMENT_RETRY_INTERVAL = baml.time.Duration.from_milliseconds(50);
