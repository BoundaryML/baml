class Artifact {
    url: string,
    sha256: string,
}

class ToolchainManifest {
    schema: int,
    version: string,
    channel: string,
    released_at: string,
    artifacts: map<string, Artifact>,
}

class WrapperManifest {
    schema: int,
    version: string,
    released_at: string,
    artifacts: map<string, Artifact>,
}

function manifest_base(override: string? = null) -> string {
    let base = (override ?? baml.env.get("BAML_MANIFEST_BASE_URL") ?? "https://pkg.boundaryml.com/manifest/v1")
        .trim();
    while (base.ends_with("/")) {
        base = base.slice(0, base.length() - 1);
    }
    ensure(secure_url(base), "manifest base URL must use HTTPS (HTTP is allowed only on loopback)");
    base
}

function secure_url(url: string) -> bool {
    url.starts_with("https://")
        || baml.regex.new("^http://(127\\.0\\.0\\.1|\\[::1\\])(:[0-9]+)?(/|$)").has_match(url)
}

function hash(data: uint8array) -> string {
    let hasher = baml.crypto.Sha256.new();
    hasher.update(data);
    hasher.finish().to_hex()
}

function cache_dir(override: string? = null) -> string {
    `${baml_home()}/manifest-cache/baml/${hash(manifest_base(override = override).to_utf8())}`
}

function targets() -> string[] {
    [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl",
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
    ]
}

function validate_artifacts(schema: int, artifacts: map<string, Artifact>) -> null {
    ensure(schema == 1, `unsupported manifest schema ${schema}; run baml self-update`);
    ensure(artifacts.keys().length() == targets().length(), "manifest has an unsupported target set");
    for (let target in targets()) {
        let artifact = artifacts.get(target);
        ensure(artifact != null, `manifest has no artifact for target ${target}`);
        if let artifact: Artifact = artifact {
            ensure(secure_url(artifact.url), "artifact URL must use HTTPS");
            ensure(
                baml.regex.new("^[0-9a-fA-F]{64}$").has_match(artifact.sha256),
                "invalid artifact sha256",
            );
        }
    }
    null
}

function fetch_text(url: string) -> string {
    let response = baml.http.fetch(url, timeout = baml.time.Duration.from_seconds(30));
    ensure(response.ok(), `failed to fetch ${url}: HTTP ${response.status_code}`);
    response.text()
}

function manifest(
    selector: string,
    force_remote: bool,
    override: string? = null,
) -> ToolchainManifest {
    validate_selector(selector);
    ensure(!is_path(selector), "local paths have no release manifest");
    baml.sys.host_target();
    let relative = if (channel(selector)) {
        `${selector}.json`
    } else {
        `version/${selector}.json`
    };
    let cached = `${cache_dir(override = override)}/${relative}`;
    let text = if (!force_remote && !channel(selector) && baml.fs.exists(cached)) {
        baml.fs.read(cached)
    } else {
        fetch_text(`${manifest_base(override = override)}/${relative}`)
    };
    let result = baml.json.decode<ToolchainManifest>(text);
    validate_artifacts(result.schema, result.artifacts);
    version(result.version);
    ensure(channel(result.channel), `invalid manifest channel ${result.channel}`);
    ensure(
        if (channel(selector)) {
            result.channel == selector
        } else {
            result.version == selector
        },
        `manifest does not match requested ${selector}: ${result.channel}/${result.version}`,
    );
    // Cache only validated metadata, separated by release source.
    baml.fs.mkdir(
        `${cache_dir(override = override)}/version`,
        baml.fs.MkdirOptions { recursive: true },
    );
    baml.fs.write(cached, text);
    baml.fs.write(`${cache_dir(override = override)}/version/${result.version}.json`, text);
    result
}

function download(artifact: Artifact) -> uint8array {
    let response = baml.http.fetch(artifact.url, timeout = baml.time.Duration.from_seconds(30));
    ensure(response.ok(), `failed to download ${artifact.url}: HTTP ${response.status_code}`);
    let data = response.bytes();
    ensure(hash(data) == artifact.sha256.to_lower_case(), `sha256 mismatch for ${artifact.url}`);
    data
}

function acquire_lock(path: string) -> baml.fs.File {
    let attempts = 0;
    while (attempts < 600) {
        let lock = baml.fs.try_lock(path);
        if let lock: baml.fs.File = lock {
            return lock;
        }
        attempts += 1;
        baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
    }
    throw baml.errors.Io { message: `timed out waiting for install lock ${path}; another installer is still running` };
}

function install_artifact(value: string, artifact: Artifact, force: bool) -> null {
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
    let data = download(artifact);
    let format = if (artifact.url.ends_with(".zip")) {
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
            { "version": value, "target": baml.sys.host_target(), "archive_url": artifact.url },
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
        _ => {
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

function record_channel(
    name: string,
    release: ToolchainManifest,
    override: string? = null,
) -> null {
    validate_state();
    let text = read_text(state_path());
    for (
        let pair in [
            ["active_version", release.version],
            ["resolved_at", release.released_at],
            ["manifest_path", `${cache_dir(override = override)}/version/${release.version}.json`],
            ["manifest_base_url", manifest_base(override = override)],
        ]
    ) {
        text = baml.toml.edit(text, ["channels", name, pair[0]], baml.json.stringify(pair[1]), []);
    }
    validate_state_text(text);
    write_atomic(state_path(), text);
    null
}

function install(
    selector: string,
    activate: bool,
    force: bool,
    force_remote: bool,
    override: string? = null,
) -> string {
    validate_selector(selector);
    ensure(!is_path(selector), `${selector} is a local path; there is nothing to install`);
    if (activate && channel(selector)) {
        validate_state();
    }
    let release = manifest(selector, force_remote, override = override);
    install_artifact(release.version, release.artifacts[baml.sys.host_target()], force);
    if (activate && channel(selector)) {
        record_channel(selector, release, override = override);
    }
    baml.io.eprintln(`installed BAML toolchain ${release.version}`);
    release.version
}

function ensure_installed(selected: Selection, override: string? = null) -> string {
    validate_selector(selected.value);
    if (is_path(selected.value)) {
        verify_local(selected.value, selected.source);
        return selected.value;
    }
    let concrete = selected.value;
    if (channel(selected.value)) {
        let active = recorded(selected.value, override = override);
        if let active: string = active {
            concrete = active;
        } else {
            concrete = install(selected.value, true, false, false, override = override);
        }
    }
    if (!installation_present(concrete)) {
        if (channel(selected.value)) {
            validate_state();
        }
        install(concrete, false, false, false, override = override);
    }
    verify_managed(concrete);
    `${toolchains()}/${concrete}/bin/${cli_name()}`
}
