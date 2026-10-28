class Artifact {
    url: string,
    sha256: string,

    function download(self) -> uint8array {
        let response = baml.http.fetch(
            self.url,
            timeout = baml.time.Duration.from_seconds(DOWNLOAD_TIMEOUT_SECONDS),
        );
        ensure(response.ok(), `failed to download ${self.url}: HTTP ${response.status_code}`);
        let data = response.bytes();
        ensure(hash(data) == self.sha256.to_lower_case(), `sha256 mismatch for ${self.url}`);
        data
    }

    function install(self, value: string, force: bool) -> null {
        version(value);
        baml.fs.mkdir(toolchains(), baml.fs.MkdirOptions { recursive: true });
        let lock = `${toolchains()}/.${value}.lock`;
        let held_lock = acquire_lock(lock);
        defer { held_lock.close(); }
        let destination = `${toolchains()}/${value}`;
        if (installation_present(value) && !force) {
            verify_managed(value);
            return null;
        }
        let data = self.download();
        let format = if (self.url.ends_with(".zip")) {
            "zip"
        } else {
            "tar.gz"
        };
        let entries = baml.archive.entries(data, format);
        for (
            let required in [
                `bin/${cli_name()}`,
                `bin/baml-pack-host${if (windows()) { ".exe" } else { "" }}`,
                "assets/baml-vscode.vsix",
                "assets/playground/index.html",
                "assets/playground/assets/index.js",
                "assets/playground/assets/index.css",
            ]
        ) {
            ensure(entries.includes(required), `archive missing expected binary or asset ${required}`);
        }
        ensure(
            !entries.includes(
                if (windows()) {
                    "bin/baml.exe"
                } else {
                    "bin/baml"
                },
            ),
            "toolchain archive contains a wrapper",
        );
        let staging = `${toolchains()}/.${value}.${baml.sys.pid()}.staging`;
        baml.fs.mkdir(staging, baml.fs.MkdirOptions { recursive: false });
        defer { baml.fs.remove_dir_all(staging); }
        baml.archive.extract(data, format, staging);
        verify_local(`${staging}/bin/${cli_name()}`, "downloaded archive");
        if (baml.fs.exists(`${staging}/VERSION`)) {
            ensure(
                baml.fs.read(`${staging}/VERSION`).trim() == value,
                "archive VERSION does not match manifest",
            );
        }
        baml.fs.write(`${staging}/VERSION`, `${value}\n`);
        baml.fs.write(
            `${staging}/install.json`,
            baml.json.stringify(
                { "version": value, "target": baml.sys.host_target(), "archive_url": self.url },
            ),
        );
        let backup = `${toolchains()}/.${value}.backup`;
        let replacing = installation_present(value);
        if (replacing) {
            if (installation_present(`.${value}.backup`)) {
                remove_installation_entry(backup);
            }
            baml.fs.rename(destination, backup);
        }
        baml.fs.rename(staging, destination) catch (error) {
            baml.errors.Io => {
                if (replacing) {
                    baml.fs.rename(backup, destination);
                }
                throw error;
            },
        };
        if (replacing) {
            remove_installation_entry(backup);
        }
        null
    }
}

class ToolchainManifest {
    schema: int,
    version: string,
    channel: ChannelName,
    released_at: string,
    artifacts: map<string, Artifact>,

    implements ReleaseMetadata {}
}

class WrapperManifest {
    schema: int,
    version: string,
    released_at: string,
    artifacts: map<string, Artifact>,

    implements ReleaseMetadata {}
}

// HTTPS or loopback HTTP only, e.g. http://127.0.0.1:8080/manifest.
function secure_url(url: string) -> bool {
    url.starts_with("https://")
        || baml.regex.new("^http://(127\\.0\\.0\\.1|\\[::1\\])(:[0-9]+)?(/|$)").has_match(url)
}

function hash(data: uint8array) -> string {
    let hasher = baml.crypto.Sha256.new();
    hasher.update(data);
    hasher.finish().to_hex()
}

const SUPPORTED_TARGETS: string[] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
];

function validate_artifacts(schema: int, artifacts: map<string, Artifact>) -> null {
    ensure(
        schema == MANIFEST_SCHEMA_VERSION,
        `unsupported manifest schema ${schema}; run baml self-update`,
    );
    ensure(
        artifacts.keys().length() == SUPPORTED_TARGETS.length(),
        "manifest has an unsupported target set",
    );
    for (let target in SUPPORTED_TARGETS) {
        let artifact = artifacts.get(target) ?? fail(`manifest has no artifact for target ${target}`);
        {
            ensure(secure_url(artifact.url), "artifact URL must use HTTPS");
            ensure(
                // A SHA-256 digest: exactly 64 hexadecimal digits.
                baml.regex.new("^[0-9a-fA-F]{64}$").has_match(artifact.sha256),
                "invalid artifact sha256",
            );
        }
    }
    null
}

function fetch_text(url: string, timeout_seconds: int = DOWNLOAD_TIMEOUT_SECONDS) -> string {
    let response = baml.http.fetch(url, timeout = baml.time.Duration.from_seconds(timeout_seconds));
    ensure(response.ok(), `failed to fetch ${url}: HTTP ${response.status_code}`);
    response.text()
}

function acquire_lock(path: string) -> baml.fs.File {
    let start = baml.time.Instant.now();
    while (start.elapsed().to_milliseconds() < INSTALL_LOCK_TIMEOUT.to_milliseconds()) {
        if let lock: baml.fs.File = baml.fs.try_lock(path) {
            return lock;
        }
        baml.sys.sleep(INSTALL_LOCK_RETRY_INTERVAL);
    }
    throw baml.errors.Io { message: `timed out waiting for install lock ${path}; another installer is still running` };
}

// Both wire manifest shapes expose this common metadata contract.
interface ReleaseMetadata {
    schema: int,
    version: string,
    artifacts: map<string, Artifact>,
}

function parse_release<Output extends ReleaseMetadata>(text: string) -> Output {
    let release = baml.json.decode<Output>(text);
    validate_artifacts(release.schema, release.artifacts);
    ManagedVersion.new(release.version);
    release
}

const INSTALL_LOCK_TIMEOUT = baml.time.Duration.from_minutes(1);
const INSTALL_LOCK_RETRY_INTERVAL = baml.time.Duration.from_milliseconds(100);
