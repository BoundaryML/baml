// The wrapper owns toolchain policy. Native code supplies ordinary OS primitives.
class WrapperError {
    message: string,
}

class Selector {
    value: string,
    origin: string,
}

class Artifact {
    url: string,
    sha256: string,
}

class Manifest {
    schema: int,
    version: string,
    channel: string?,
    released_at: string,
    artifacts: map<string, Artifact>,
}

function Main() -> void {
    entry(true)
}

function NoSelfUpdateMain() -> void {
    entry(false)
}

function entry(self_update_enabled: bool) -> void {
    let raw = baml.sys.argv();
    let args = raw.slice(2, raw.length());
    {
        dispatch(args, self_update_enabled);
    } catch (error) {
        let e: WrapperError => {
            baml.io.eprintln(e.message);
            baml.sys.exit(1);
        },
        let e: baml.errors.Io => {
            baml.io.eprintln(e.message);
            baml.sys.exit(1);
        },
        _ => {
            baml.io.eprintln(baml.json.to_string(error));
            baml.sys.exit(1);
        },
    }
}

function fail(message: string) -> never {
    throw WrapperError { message }
}

function join(parts: string[]) -> string {
    baml.path.join(parts)
}

function home() -> string {
    let override = (baml.env.get("BAML_HOME") ?? "").trim();
    if (override != "") {
        return baml.path.absolute(override);
    }
    join([baml.sys.home_dir() ?? fail("Cannot locate home directory; set BAML_HOME."), ".baml"])
}

function base_url(override: string?) -> string {
    let supplied = (override ?? baml.env.get("BAML_MANIFEST_BASE_URL") ?? "").trim();
    let base = if (supplied == "") {
        "https://pkg.boundaryml.com/manifest/v1"
    } else {
        supplied
    };
    while (base.ends_with("/")) {
        base = base.slice(0, base.length() - 1);
    }
    base
}

function cache_dir(base: string) -> string {
    let key = if (base == "https://pkg.boundaryml.com/manifest/v1") {
        "prod"
    } else {
        baml.crypto.sha256(base.to_utf8())
    };
    join([home(), "manifest-cache", key])
}

function manifest_path(selector: string, base: string) -> string {
    if (channel(selector)) {
        join([cache_dir(base), selector + ".json"])
    } else {
        join([cache_dir(base), "version", selector + ".json"])
    }
}

function channel(value: string) -> bool {
    value == "canary" || value == "nightly"
}

function local(value: string) -> bool {
    value.starts_with("~")
        || value.starts_with(".")
        || value.includes("/")
        || (baml.sys.platform() == "windows" && value.includes("\\"))
}

function version_name(value: string) -> void {
    if (
        value == ""
            || value == "."
            || value == ".."
            || value.includes("/")
            || value.includes("\\")
            || value.includes(":")
    ) {
        fail("Invalid toolchain version: " + value);
    }
}

function executable(name: string) -> string {
    if (baml.sys.platform() == "windows") {
        name + ".exe"
    } else {
        name
    }
}

function normalize(value: string, base: string) -> string {
    if (!local(value)) {
        version_name(value);
        return value;
    }
    let path = baml.path.absolute(value, base = base);
    if (baml.sys.platform() == "windows" && !path.ends_with(".exe") && baml.fs.exists(path + ".exe")) {
        path = path + ".exe";
    }
    path
}

function read_text(path: string) -> string {
    if (baml.fs.exists(path)) {
        baml.fs.read(path)
    } else {
        ""
    }
}

function table(path: string) -> baml.toml.Table {
    baml.toml.Table.parse(read_text(path))
}

function nested(t: baml.toml.Table, keys: string[]) -> baml.toml.Table {
    let current = t;
    for (let key in keys) {
        current
            = match (current.items.get(key)) {
                let child: baml.toml.Table => child,
                null => baml.toml.Table { items: {} },
                _ => fail("Expected TOML table: " + key),
            };
    }
    current
}

function setting(t: baml.toml.Table, keys: string[], key: string) -> string? {
    match (nested(t, keys).items.get(key)) {
        let value: string => value,
        null => null,
        _ => fail("Expected string configuration value: " + key),
    }
}

function edit(path: string, keys: string[], values: map<string, string>) -> void {
    let lock = baml.fs.lock(path + ".lock", 30000);
    defer { lock.close() }
    let config = baml.toml.Table.parse(read_text(path));
    let target = config;
    for (let key in keys) {
        target = target.table(key);
    }
    for (let key in values.keys()) {
        target.set(key, values[key]);
    }
    baml.fs.write_atomic(path, config.to_string().to_utf8());
}

function pin_selector(path: string, key: string, value: string) -> void {
    let lock = baml.fs.lock(path + ".lock", 30000);
    defer { lock.close() }
    let config = baml.toml.Table.parse(read_text(path));
    let toolchain = config.table("toolchain");
    // The selected key's comment follows its explicit rename. Conflicting
    // selectors are removed before renaming, so the destination is available.
    let original: string? = null;
    for (let name in ["version", "channel", "path"]) {
        if (original == null && toolchain.get(name) != null) {
            original = name;
        }
    }
    for (let name in ["version", "channel", "path"]) {
        if (name != original) {
            toolchain.remove(name);
        }
    }
    match (original) {
        let from: string => {
            toolchain.rename(from, key);
        },
        null => {},
    }
    toolchain.set(key, value);
    baml.fs.write_atomic(path, config.to_string().to_utf8());
}

function config_selector() -> string {
    let value = (setting(table(join([home(), "config.toml"])), ["default"], "selector") ?? "canary").trim();
    let value = if (value == "") {
        "canary"
    } else {
        value
    };
    if (
        local(value)
            && !baml.path.is_absolute(value)
            && !value.starts_with("~/")
            && !value.starts_with("~\\")
    ) {
        fail("Global default.selector must be an absolute path. Run: baml toolchain use <path>");
    }
    normalize(value, baml.sys.current_dir())
}

function home_boundary() -> string? {
    match (baml.sys.home_dir()) {
        let path: string => {
            let canonical = {
                baml.fs.canonicalize(path)
            } catch (e) {
                baml.errors.Io => baml.path.absolute(path),
            };
            canonical
        },
        null => null,
    }
}

function project_manifest() -> string? {
    let dir = baml.sys.current_dir();
    let home_dir = home_boundary();
    while (true) {
        let candidate = join([dir, "baml.toml"]);
        if (baml.fs.exists(candidate)) {
            return candidate;
        }
        if (home_dir == dir) {
            return null;
        }
        match (baml.path.parent(dir)) {
            let parent: string => {
                if (parent == dir) {
                    return null;
                }
                dir = parent;
            },
            null => {
                return null;
            },
        }
    }
}

function active() -> Selector {
    let env = (baml.env.get("BAML_VERSION") ?? "").trim();
    if (env != "") {
        return Selector { value: normalize(env, baml.sys.current_dir()), origin: "from $BAML_VERSION" };
    }
    // Walk past manifests without a selector, stopping at the user's home/root.
    let dir = baml.sys.current_dir();
    let home_dir = home_boundary();
    while (true) {
        let path = join([dir, "baml.toml"]);
        if (baml.fs.exists(path)) {
            let config = table(path);
            let selected = setting(config, ["toolchain"], "version") ?? setting(config, ["toolchain"], "channel")
                ?? setting(config, ["toolchain"], "path");
            if let value: string = selected {
                return Selector { value: normalize(value, dir), origin: "from " + path };
            }
        }
        if (home_dir == dir) {
            break;
        }
        match (baml.path.parent(dir)) {
            let parent: string => {
                if (parent == dir) {
                    break;
                }
                dir = parent;
            },
            null => {
                break;
            },
        }
    }
    Selector { value: config_selector(), origin: "" }
}

function concrete(selector: string, base: string) -> string {
    if (!channel(selector)) {
        version_name(selector);
        return selector;
    }
    let state = table(join([home(), "state.toml"]));
    let source = setting(state, ["channels", selector], "manifest_base_url");
    if (source != null && source != base) {
        fail(
            "Channel "
                + selector
                + " was selected from a different manifest source. Run: baml toolchain use "
                + selector,
        );
    }
    let version = setting(state, ["channels", selector], "active_version")
        ?? fail("No BAML toolchain is installed. Run: baml toolchain use " + selector);
    version_name(version);
    version
}

function toolchain_dir(version: string) -> string {
    version_name(version);
    join([home(), "toolchains", version])
}

function cli_path(version: string) -> string {
    join([toolchain_dir(version), "bin", executable("baml-cli")])
}

function verify_local(path: string) -> void {
    let info = baml.fs.metadata(path);
    if (!info.is_file || !info.is_executable) {
        fail("Toolchain is not an executable file: " + path);
    }
    if (baml.fs.canonicalize(path) == baml.fs.canonicalize(baml.sys.current_exe())) {
        fail("A toolchain cannot point to the BAML wrapper itself: " + path);
    }
}

function verify_installed(version: string) -> void {
    let path = cli_path(version);
    if (!baml.fs.exists(path)) {
        fail("BAML toolchain " + version + " is not installed. Run: baml toolchain install " + version);
    }
    if (read_text(join([toolchain_dir(version), "VERSION"])).trim() != version) {
        fail(
            "BAML toolchain "
                + version
                + " is corrupt. Run: baml toolchain install "
                + version
                + " --force",
        );
    }
    verify_local(path);
}

function fresh(path: string) -> bool {
    if (!baml.fs.exists(path)) {
        return false;
    }
    match (baml.fs.metadata(path).modified_ms) {
        let ms: int => baml.time.Instant.now().to_timestamp_milliseconds().to_int() - ms <= 86400000,
        null => false,
    }
}

function fetch_bytes(url: string, seconds: int) -> uint8array {
    let response = baml.http.fetch(url, timeout = baml.time.Duration.from_seconds(seconds));
    if (response.status_code < 200 || response.status_code >= 300) {
        fail("HTTP " + response.status_code.to_string() + " fetching " + url);
    }
    response.bytes()
}

function validate_manifest(manifest: Manifest) -> void {
    if (manifest.schema != 1) {
        fail("Unsupported manifest schema: " + manifest.schema.to_string());
    }
    version_name(manifest.version);
    let targets = [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-musl",
        "aarch64-pc-windows-msvc",
        "x86_64-pc-windows-msvc",
    ];
    if (manifest.artifacts.length() != targets.length()) {
        fail("Manifest must contain all release platforms.");
    }
    for (let target in targets) {
        let artifact = manifest.artifacts.get(target) ?? fail("Manifest is missing platform " + target);
        if (!artifact.url.starts_with("https://")) {
            fail("Artifact URL must use HTTPS: " + artifact.url);
        }
        if (artifact.sha256.length() != 64 || uint8array.from_hex(artifact.sha256).length() != 32) {
            fail("Invalid artifact SHA-256.");
        }
    }
}

function fetch_manifest(selector: string, base: string, force: bool, seconds: int) -> Manifest {
    version_name(selector);
    let path = manifest_path(selector, base);
    let cached = !force && baml.fs.exists(path) && (!channel(selector) || fresh(path));
    let remote = if (channel(selector)) {
        base + "/" + selector + ".json"
    } else {
        base + "/version/" + selector + ".json"
    };
    let text = if (cached) {
        baml.fs.read(path)
    } else {
        fetch_bytes(remote, seconds).to_string()
    };
    let manifest = baml.json.from_string<Manifest>(text);
    validate_manifest(manifest);
    if (!channel(selector) && manifest.version != selector) {
        fail("Manifest version does not match requested toolchain.");
    }
    if (channel(selector) && manifest.channel != selector) {
        fail("Manifest channel does not match requested toolchain.");
    }
    if (!cached) {
        baml.fs.write_atomic(path, text.to_utf8());
    }
    let version_path = manifest_path(manifest.version, base);
    if (!baml.fs.exists(version_path)) {
        baml.fs.write_atomic(version_path, text.to_utf8());
    }
    manifest
}

function download(artifact: Artifact) -> uint8array {
    let bytes = fetch_bytes(artifact.url, 30);
    if (baml.crypto.sha256(bytes) != artifact.sha256.to_lower_case()) {
        fail("Artifact checksum mismatch: " + artifact.url);
    }
    bytes
}

function archive_format() -> "zip" | "tar.gz" {
    if (baml.sys.platform() == "windows") {
        "zip"
    } else {
        "tar.gz"
    }
}

function unique() -> string {
    baml.random.SystemRandom {}.random(16).to_hex()
}

function publish_install(manifest: Manifest, force: bool) -> void {
    let root = join([home(), "toolchains"]);
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let destination = toolchain_dir(manifest.version);
    let lock = baml.fs.lock(join([home(), "locks", manifest.version + ".lock"]), 30000);
    defer { lock.close() }
    if (baml.fs.exists(destination) && !force) {
        verify_installed(manifest.version);
        return;
    }
    let artifact = manifest.artifacts.get(baml.sys.host_target()) ?? fail("Unsupported host platform.");
    let staging = join([root, ".staging-" + unique()]);
    defer { baml.fs.remove_dir_all(staging); }
    baml.archive.extract(download(artifact), archive_format(), staging);
    let required = [
        join(["bin", executable("baml-cli")]),
        join(["bin", executable("baml-pack-host")]),
        "assets/baml-vscode.vsix",
        "assets/playground/index.html",
        "assets/playground/assets/index.js",
        "assets/playground/assets/index.css",
    ];
    for (let file in required) {
        let path = join([staging, file]);
        if (!baml.fs.exists(path) || !baml.fs.metadata(path).is_file) {
            fail("Toolchain archive is missing " + file);
        }
    }
    if (baml.fs.exists(join([staging, "bin", executable("baml")]))) {
        fail("Toolchain archive unexpectedly contains the wrapper.");
    }
    baml.fs.chmod(join([staging, "bin", executable("baml-cli")]), 0o755);
    baml.fs.chmod(join([staging, "bin", executable("baml-pack-host")]), 0o755);
    baml.fs.write_atomic(join([staging, "VERSION"]), (manifest.version + "\n").to_utf8());
    baml.fs.write_atomic(join([staging, "install.json"]), baml.json.to_string(manifest).to_utf8());
    let backup = join([root, ".backup-" + unique()]);
    let replacing = baml.fs.exists(destination);
    if (replacing) {
        baml.fs.rename(destination, backup);
    }
    {
        baml.fs.rename(staging, destination);
    } catch (error) {
        _ => {
            if (replacing) {
                baml.fs.rename(backup, destination);
            }
            throw error;
        },
    };
    if (replacing) {
        baml.fs.remove_dir_all(backup);
    }
}

function install(
    selector: string,
    base: string,
    activate: bool,
    force: bool,
    remote: bool,
) -> void {
    if (local(selector)) {
        fail(selector + " is a local path; use baml toolchain use <path>.");
    }
    let manifest = fetch_manifest(selector, base, remote, 30);
    publish_install(manifest, force);
    if (activate && channel(selector)) {
        edit(
            join([home(), "state.toml"]),
            ["channels", selector],
            {
                "active_version": manifest.version,
                "resolved_at": manifest.released_at,
                "manifest_path": manifest_path(manifest.version, base),
                "manifest_base_url": base,
            },
        );
    }
    baml.io.println("installed BAML toolchain " + manifest.version);
}

function prepare(selector: string, base: string) -> string {
    let value = normalize(selector, baml.sys.current_dir());
    if (local(value)) {
        verify_local(value);
    } else if (channel(value)) {
        install(value, base, true, false, false);
    } else if (!baml.fs.exists(cli_path(value))) {
        install(value, base, false, false, false);
    } else {
        verify_installed(value);
    }
    value
}

function outdated(selector: string, version: string, base: string) -> void {
    if (!channel(selector)) {
        return;
    }
    // A damaged cache must never stop the CLI from launching.
    {
        let path = manifest_path(selector, base);
        if (
            baml.fs.exists(path)
                && baml.json.from_string<Manifest>(baml.fs.read(path)).version != version
        ) {
            baml.io.eprintln(
                "warning: Your version of baml for toolchain: "
                    + selector
                    + " is outdated. Update it with baml toolchain update.",
            );
        }
    } catch (e) {
        _ => {}
    }
}

function auto_check() -> bool {
    match (nested(table(join([home(), "config.toml"])), ["update"]).items.get("auto_check")) {
        let enabled: bool => enabled,
        null => true,
        _ => fail("update.auto_check must be a boolean."),
    }
}

function passthrough(args: string[]) -> never {
    let selected = active();
    let base = base_url(null);
    if (local(selected.value)) {
        verify_local(selected.value);
        let status = baml.sys.run(
            selected.value,
            args = args,
            env = {
                "BAML_WRAPPER_EXEC": "1",
                "BAML_WRAPPER_LOCAL_TOOLCHAIN": selected.value,
                "BAML_WRAPPER_RESOLVED_TOOLCHAIN": null,
            },
        );
        baml.sys.exit(status.exit_code);
    }
    let version = concrete(selected.value, base);
    verify_installed(version);
    outdated(selected.value, version, base);
    let cache = manifest_path(selected.value, base);
    let marker = cache + ".last-check";
    if (channel(selected.value) && auto_check() && baml.fs.exists(cache) && !fresh(marker)) {
        // The CLI runs immediately while the network check proceeds; failures are best effort.
        {
            baml.fs.write_atomic(marker, b"checked");
        } catch (e) {
            _ => {}
        };
        let refresh = spawn {
            {
                fetch_manifest(selected.value, base, true, 5);
            } catch (e) {
                _ => {}
            }
        };
        let status = baml.sys.run(
            cli_path(version),
            args = args,
            env = {
                "BAML_WRAPPER_EXEC": "1",
                "BAML_WRAPPER_RESOLVED_TOOLCHAIN": version,
                "BAML_WRAPPER_LOCAL_TOOLCHAIN": null,
            },
        );
        await refresh;
        outdated(selected.value, version, base);
        baml.sys.exit(status.exit_code);
    }
    let status = baml.sys.run(
        cli_path(version),
        args = args,
        env = {
            "BAML_WRAPPER_EXEC": "1",
            "BAML_WRAPPER_RESOLVED_TOOLCHAIN": version,
            "BAML_WRAPPER_LOCAL_TOOLCHAIN": null,
        },
    );
    baml.sys.exit(status.exit_code)
}

function local_version(path: string) -> string {
    {
        let output = baml.sys.run(
            path,
            args = ["--version"],
            timeout = baml.time.Duration.from_seconds(3),
            capture_output = true,
        );
        if (!output.ok()) {
            return "version unknown";
        }
        let first = (output.stdout.to_string().split("\n").at(0) ?? "").trim();
        let words = first.split(" ");
        if (words.at(0) == "baml" || words.at(0) == "baml-cli") {
            return words.at(1) ?? "version unknown";
        }
        if (first == "") {
            "version unknown"
        } else {
            first
        }
    } catch (e) {
        _ => "version unknown",
    }
}

function print_version() -> void {
    baml.io.println("baml wrapper " + WrapperVersion());
    {
        let selected = active();
        if (local(selected.value)) {
            verify_local(selected.value);
            let version = local_version(selected.value);
            baml.io.println("baml toolchain " + version + " (local: " + selected.value + ")");
        } else {
            let version = concrete(selected.value, base_url(null));
            verify_installed(version);
            baml.io.println(
                "baml toolchain "
                    + version
                    + if (selected.origin == "") {
                        ""
                    } else {
                        " (" + selected.origin + ")"
                    },
            );
        }
    } catch (e) {
        let error: WrapperError => {
            baml.io.println("baml toolchain not resolved\n" + error.message);
        },
        _ => {
            baml.io.println("baml toolchain not resolved");
        },
    }
}

function list() -> void {
    baml.io.println("default: " + config_selector());
    let root = join([home(), "toolchains"]);
    let versions: string[] = [];
    if (baml.fs.exists(root)) {
        for (let entry in baml.fs.read_dir(root)) {
            if (entry.is_dir && !entry.name.starts_with(".")) {
                versions.push(entry.name);
            }
        }
    }
    for (let version in versions.sort()) {
        baml.io.println("  " + version);
    }
    baml.io.println("Remote versions were not checked.");
}

function status(base: string) -> void {
    let selected = active();
    baml.io.println("selector: " + selected.value);
    if (selected.origin != "") {
        baml.io.println("  " + selected.origin);
    }
    if (local(selected.value)) {
        verify_local(selected.value);
        baml.io.println("local: " + selected.value);
        return;
    }
    let version = concrete(selected.value, base);
    verify_installed(version);
    baml.io.println("active: " + version);
    if (channel(selected.value)) {
        let latest = fetch_manifest(selected.value, base, true, 30).version;
        baml.io.println("latest: " + latest);
        if (latest != version) {
            baml.io.println("Run: baml toolchain update");
        }
    } else {
        for (let name in ["canary", "nightly"]) {
            baml.io.println(name + ": " + fetch_manifest(name, base, true, 30).version);
        }
    }
}

function toolchain(args: string[]) -> void {
    let filtered: string[] = [];
    let override: string? = null;
    let i = 0;
    while (i < args.length()) {
        let arg = args.at(i) ?? "";
        if (arg == "--manifest-base-url") {
            i = i + 1;
            override = args.at(i) ?? fail("--manifest-base-url requires a value");
        } else {
            filtered.push(arg);
        }
        i = i + 1;
    }
    let base = base_url(override);
    let command = filtered.at(0) ?? "help";
    let value = filtered.at(1);
    if ((command == "use" || command == "pin" || command == "uninstall") && filtered.length() != 2) {
        fail("usage: baml toolchain " + command + " <selector>");
    }
    if ((command == "update" || command == "status" || command == "list") && filtered.length() != 1) {
        fail("Unexpected arguments to baml toolchain " + command);
    }
    match (command) {
        "help" | "--help" | "-h" => {
            baml.io.println(
                "BAML toolchain management\n\n  use <canary|nightly|version|path>\n  pin <canary|nightly|version|path>\n  install <canary|nightly|version> [--force]\n  update\n  status\n  list\n  uninstall <version>\n\nOptions: --manifest-base-url <url>",
            );
        },
        "use" => {
            let selected = prepare(value ?? fail("usage: baml toolchain use <selector>"), base);
            edit(join([home(), "config.toml"]), ["default"], { "selector": selected });
            baml.io.println("selected BAML toolchain " + selected);
        },
        "pin" => {
            if (filtered.length() != 2) {
                fail("usage: baml toolchain pin <selector>");
            }
            let path = project_manifest()
                ?? fail("No baml.toml found. Run this command inside a BAML project.");
            let selected = prepare(value ?? fail("usage: baml toolchain pin <selector>"), base);
            let key = if (channel(selected)) {
                "channel"
            } else if (local(selected)) {
                "path"
            } else {
                "version"
            };
            pin_selector(path, key, selected);
            baml.io.println("pinned BAML toolchain " + selected + " in " + path);
        },
        "install" => {
            let force = false;
            for (let arg in filtered.slice(2, filtered.length())) {
                if (arg == "--force") {
                    force = true;
                } else {
                    fail("Unexpected argument: " + arg);
                }
            }
            install(value ?? fail("usage: baml toolchain install <selector>"), base, false, force, false);
        },
        "update" => {
            let selected = config_selector();
            if (channel(selected)) {
                install(selected, base, true, false, true);
            } else {
                baml.io.println(
                    "Default toolchain is pinned to " + selected + "; no channel to update.",
                );
            }
        },
        "status" => {
            status(base);
        },
        "list" => {
            list();
        },
        "uninstall" => {
            let version = value ?? fail("usage: baml toolchain uninstall <version>");
            version_name(version);
            let lock = baml.fs.lock(join([home(), "locks", version + ".lock"]), 30000);
            defer { lock.close() }
            baml.fs.remove_dir_all(toolchain_dir(version));
            baml.io.println("uninstalled BAML toolchain " + version);
        },
        _ => {
            fail("Unknown toolchain command: " + command);
        },
    }
}

function self_update() -> void {
    let manifest = baml.json.from_string<Manifest>(fetch_bytes(base_url(null) + "/wrapper.json", 30).to_string());
    validate_manifest(manifest);
    let artifact = manifest.artifacts.get(baml.sys.host_target()) ?? fail("Unsupported host platform.");
    let bytes = baml.archive.read_file(download(artifact), archive_format(), "bin/" + executable("baml"));
    let current = baml.sys.current_exe();
    let lock = baml.fs.lock(current + ".update.lock", 30000);
    defer { lock.close() }
    let tmp = current + ".update-" + unique() + ".exe";
    baml.fs.write_atomic(tmp, bytes);
    baml.fs.chmod(tmp, 0o755);
    if (baml.sys.platform() == "windows") {
        // A helper running from `current` would keep that executable locked.
        let helpers = join([home(), "update-helpers"]);
        baml.fs.mkdir(helpers, baml.fs.MkdirOptions { recursive: true });
        for (let old in baml.fs.read_dir(helpers)) {
            if (!old.is_dir) {
                {
                    baml.fs.remove(join([helpers, old.name]));
                } catch (e) {
                    baml.errors.Io => {}
                };
            }
        }
        let helper = join([helpers, unique() + ".exe"]);
        let file = baml.fs.open(current, "r");
        let helper_bytes = file.bytes();
        file.close();
        baml.fs.write_atomic(helper, helper_bytes);
        let _ = baml.sys.subprocess(
            helper,
            args = ["--replace", tmp, current],
            detached = true,
            stdin = "ignore",
            stdout = "ignore",
            stderr = "ignore",
        );
        baml.io.println("updating BAML wrapper to " + manifest.version);
    } else {
        defer { if (baml.fs.exists(tmp)) { baml.fs.remove(tmp); } }
        baml.fs.rename(tmp, current);
        baml.io.println("updated BAML wrapper to " + manifest.version);
    }
}

function replace(args: string[]) -> void {
    if (args.length() != 2) {
        fail("usage: baml --replace <tmp> <current>");
    }
    let tmp = args.at(0) ?? fail("Missing replacement path");
    let current = args.at(1) ?? fail("Missing executable path");
    let lock = baml.fs.lock(current + ".update.lock", 30000);
    defer { lock.close() }
    let attempt = 0;
    while (attempt < 60) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(250));
        let replaced = {
            baml.fs.rename(tmp, current);
            true
        } catch (e) {
            baml.errors.Io => false
        };
        if (replaced) {
            return;
        }
        attempt = attempt + 1;
    }
    fail("Failed to replace wrapper executable: " + current);
}

function dispatch(args: string[], self_update_enabled: bool) -> void {
    match (args.at(0)) {
        "--version" | "-V" => {
            print_version();
        },
        "toolchain" => {
            toolchain(args.slice(1, args.length()));
        },
        "self-update" => {
            if (!self_update_enabled) {
                fail("self-update is disabled in this build.\nUpdate BAML with your package manager.");
            }
            if (args.at(1) == "--help" || args.at(1) == "-h") {
                baml.io.println("BAML wrapper self-update\n\nUsage: baml self-update");
            } else {
                self_update();
            }
        },
        "--replace" => {
            if (!self_update_enabled || baml.sys.platform() != "windows") {
                fail("Executable replacement is unavailable in this build.");
            }
            replace(args.slice(1, args.length()));
        },
        _ => {
            passthrough(args);
        },
    }
}
