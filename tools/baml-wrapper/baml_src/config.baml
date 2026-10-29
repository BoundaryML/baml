function ensure(ok: bool, message: string) -> null throws baml.errors.InvalidArgument {
    if (!ok) {
        throw baml.errors.InvalidArgument { message: message };
    }
    null
}

function windows() -> bool {
    baml.sys.host_target().includes("windows")
}

function cli_name() -> string {
    if (windows()) {
        "baml-cli.exe"
    } else {
        "baml-cli"
    }
}

function home() -> string {
    baml.env.get("HOME") ?? baml.env.get("USERPROFILE") ?? "."
}

function baml_home() -> string {
    let override = (baml.env.get("BAML_HOME") ?? "").trim();
    if (override) {
        return override;
    }
    `${home()}/.baml`
}

function config_path() -> string {
    `${baml_home()}/config.toml`
}

function state_path() -> string {
    `${baml_home()}/state.toml`
}

function toolchains() -> string {
    `${baml_home()}/toolchains`
}

function channel(value: string) -> bool {
    CHANNEL_NAMES.some((name) -> bool {
        name == value
    })
}

function is_path(value: string) -> bool {
    value.starts_with("~")
        || value.starts_with(".")
        || value.includes("/")
        || (windows() && value.includes("\\"))
}

function version(value: string) -> null {
    ManagedVersion.new(value);
    null
}

function validate_selector(value: string) -> null {
    if (!channel(value) && !is_path(value)) {
        version(value);
    }
    null
}

// Gap: the stdlib has no path object. Keep path operations here and preserve
// symlink-sensitive `..` components instead of normalizing them lexically.
function parent(raw: string) -> string {
    let path = raw.replace_all("\\", "/");
    while (path.ends_with("/") && path.length() > 1) {
        path = path.slice(0, path.length() - 1);
    }
    let parts = path.split("/");
    if (parts.length() <= 1) {
        return path;
    }
    let result = parts.slice(0, parts.length() - 1).join("/");
    if (!result) {
        return "/";
    }
    if (result.ends_with(":")) {
        return `${result}/`;
    }
    result
}

function absolute(raw: string, base: string) -> string {
    let path = raw.trim();
    if (path == "~" || path.starts_with("~/")) {
        path = `${home()}${path.slice(1, path.length())}`;
    }
    // A Windows absolute drive path, e.g. C:\baml\baml-cli.exe or C:/baml/baml-cli.exe.
    let absolute_path = path.starts_with("/")
        || (
            windows()
                && (path.starts_with("\\\\") || baml.regex.new("^[A-Za-z]:[/\\\\]").has_match(path))
        );
    if (!absolute_path) {
        path = `${base}/${path}`;
    }
    if (windows() && !baml.fs.exists(path) && !path.ends_with(".exe") && baml.fs.exists(`${path}.exe`)) {
        path = `${path}.exe`;
    }
    path
}

function normalized(value: string, base: string) -> string {
    if (is_path(value)) {
        absolute(value, base)
    } else {
        value.trim()
    }
}

function read_text(path: string) -> string {
    if (!baml.fs.exists(path)) {
        return "";
    }
    baml.fs.read(path)
}

function document(path: string) -> baml.toml.Table {
    baml.toml.Table.parse(read_text(path)) catch (error) {
        baml.toml.ParseError | baml.errors.Io => {
            let message = match (error) {
                baml.toml.ParseError { message } | baml.errors.Io { message } => message,
            };
            throw baml.errors.InvalidArgument { message: `${path}: ${message}` };
        },
    }
}

function table_at(table: baml.toml.Table, key: string, origin: string) -> baml.toml.Table? {
    match (table.items.get(key)) {
        null => null,
        let value: baml.toml.Table => value,
        _ => throw baml.errors.InvalidArgument { message: `${origin}: ${key} must be a table` },
    }
}

function string_at(table: baml.toml.Table, key: string, origin: string) -> string? {
    match (table.items.get(key)) {
        null => null,
        let value: string => value,
        _ => throw baml.errors.InvalidArgument { message: `${origin}: ${key} must be a string` },
    }
}

function canonical_directory(path: string) -> string {
    let resolved = baml.fs.canonicalize(path);
    // The parent walker uses ordinary separators; Windows verbatim paths do not.
    if (windows() && resolved.starts_with("\\\\?\\UNC\\")) {
        return `//${resolved.slice(8, resolved.length()).replace_all("\\", "/")}`;
    }
    if (windows() && resolved.starts_with("\\\\?\\")) {
        resolved = resolved.slice(4, resolved.length());
    }
    resolved.replace_all("\\", "/")
}

function closest_manifest(start: string? = null) -> string? {
    let dir = canonical_directory(start ?? baml.sys.cwd());
    let boundary = if (baml.fs.exists(home())) {
        canonical_directory(home())
    } else {
        home()
    };
    while (true) {
        let path = `${dir}/baml.toml`;
        if (baml.fs.exists(path)) {
            return path;
        }
        let next = parent(dir);
        if (dir == next || dir.replace_all("\\", "/") == boundary.replace_all("\\", "/")) {
            return null;
        }
        dir = next;
    }
}

function validate_channel(table: baml.toml.Table, origin: string) -> string {
    let active = string_at(table, "active_version", origin) ?? fail(`${origin}: active_version is required`);
    version(active);
    ensure(
        string_at(table, "resolved_at", origin) != null
            && string_at(table, "manifest_path", origin) != null,
        `${origin}: incomplete channel state`,
    );
    string_at(table, "manifest_base_url", origin);
    active
}

function write_atomic(path: string, text: string) -> null {
    baml.fs.mkdir(parent(path), baml.fs.MkdirOptions { recursive: true });
    let tmp = `${path}.${baml.sys.pid()}.${int.random(0, int.max_value())}.tmp`;
    defer {
        if (baml.fs.exists(tmp)) {
            baml.fs.remove(tmp);
        }
    }
    // Keep contents private before writing, then retain the destination's mode.
    baml.fs.write(tmp, "");
    baml.fs.chmod(tmp, 384);
    baml.fs.write(tmp, text);
    if (baml.fs.exists(path)) {
        baml.fs.chmod(tmp, baml.fs.stat(path).mode);
    }
    baml.fs.rename(tmp, path);
    null
}

function selection_document(
    path: string,
    original: string,
    selected: string,
    global: bool,
) -> string {
    if (global) {
        UserConfiguration.new(original);
    } else {
        ProjectConfiguration.new(path, original);
    }
    let updated = if (global) {
        baml.toml.edit(original, ["default", "selector"], baml.json.stringify(selected), [])
    } else {
        let key = if (is_path(selected)) {
            "path"
        } else if (channel(selected)) {
            "channel"
        } else {
            "version"
        };
        baml.toml.edit(
            original,
            ["toolchain", key],
            baml.json.stringify(selected),
            ["version", "channel", "path"],
        )
    };
    if (global) {
        UserConfiguration.new(updated);
    } else {
        ProjectConfiguration.new(path, updated);
    }
    updated
}

function verify_local(path: string, source: string) -> null {
    ensure(baml.fs.exists(path), `toolchain binary not found: ${path}\n  set by ${source}`);
    let metadata = baml.fs.stat(path);
    ensure(!metadata.is_dir, `toolchain path is a directory: ${path}\n  set by ${source}`);
    ensure(
        metadata.is_file && metadata.executable,
        `toolchain is not executable: ${path}\n  set by ${source}`,
    );
    ensure(
        baml.fs.canonicalize(path) != baml.fs.canonicalize(baml.sys.executable()),
        `toolchain path points at the baml wrapper itself: ${path}`,
    );
    null
}

function verify_managed(value: string) -> null {
    version(value);
    let installation = `${toolchains()}/${value}`;
    let repair = `BAML toolchain ${value} is damaged. Run: baml toolchain install ${value} --force`;
    ensure(baml.fs.exists(installation), repair);
    ensure(baml.fs.stat(installation).is_dir, repair);
    ensure(baml.fs.exists(`${installation}/VERSION`), repair);
    ensure(baml.fs.read(`${installation}/VERSION`).trim() == value, repair);
    verify_local(`${installation}/bin/${cli_name()}`, repair);
    null
}

class ProjectShape {
    package: PackageShape?,
    dependencies: map<string, DependencyShape>?,
    scripts: map<string, string | string[]>?,
    generators: map<string, GeneratorShape>? @alias("generator"),
    test_settings: TestShape? @alias("test"),
    pack: PackShape?,
    bridge: PackShape?,
    boundary: BoundaryShape?,
}

class PackageShape {
    name: string?,
    prelude: bool?,
}

class DependencyShape {
    path: string,
}

class GeneratorShape {
    output_type: string?,
    naming_convention: string?,
    output_dir: string?,
    sdk_import_path: string?,
    max_typed_union_arity: int?,
}

class TestShape {
    default: string?,
    profiles: map<string, TestProfileShape>?,
}

class TestProfileShape {
    args: string[]?,
}

class PackShape {
    telemetry_environment: string?,
}

class BoundaryShape {
    project: string?,
    api_url: string?,
}

function installation_present(value: string) -> bool {
    if (baml.fs.exists(`${toolchains()}/${value}`)) {
        return true;
    }
    if (!baml.fs.exists(toolchains())) {
        return false;
    }
    baml.fs.read_dir(toolchains()).some((entry) -> bool {
        entry.name == value
    })
}

function validate_strings(table: baml.toml.Table, keys: string[], origin: string) -> null {
    for (let key in keys) {
        string_at(table, key, origin);
    }
    null
}

function validate_string_items(items: baml.toml.Item[], origin: string) -> null {
    for (let item in items) {
        match (item) {
            string => {},
            _ => {
                ensure(false, `${origin}: expected a string array`);
            },
        }
    }
    null
}

function remove_installation_entry(path: string) -> null {
    baml.fs.remove_dir_all(path) catch (error) {
        baml.errors.Io => {
            baml.fs.remove(path);
        }
    };
    null
}

class ToolchainResolution {
    schema_version: int,
    selector: string,
    version: string?,
    source: string,
    wrapper_path: string,
    toolchain_path: string,
    managed: bool,
    installed: bool,
    available_releases: map<string, string>,
}
