class Selection {
    value: string,
    source: string,
}

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
    if (override != "") {
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
    value == "canary" || value == "nightly"
}

function is_path(value: string) -> bool {
    value.starts_with("~")
        || value.starts_with(".")
        || value.includes("/")
        || (windows() && value.includes("\\"))
}

function version(value: string) -> null {
    let number = "(0|[1-9][0-9]*)";
    let identifier = "(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)";
    let pattern = `^${number}\\.${number}\\.${number}(-${identifier}(\\.${identifier})*)?(\\+[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?$`;
    ensure(baml.regex.new(pattern).has_match(value), `invalid managed toolchain version ${value}`);
    let core = value.split("-")[0].split("+")[0];
    for (let component in core.split(".")) {
        ensure(
            component.length() <= 20 && bigint.parse(component) <= 18446744073709551615n,
            `invalid managed toolchain version ${value}`,
        );
    }
    null
}

function validate_selector(value: string) -> null {
    if (!channel(value) && !is_path(value)) {
        version(value);
    }
    null
}

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
    if (result == "") {
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
        path = home() + path.slice(1, path.length());
    }
    let absolute_path = path.starts_with("/")
        || (
            windows()
                && (path.starts_with("\\\\") || baml.regex.new("^[A-Za-z]:[/\\\\]").has_match(path))
        );
    if (!absolute_path) {
        path = `${base}/${path}`;
    }
    if (windows() && !baml.fs.exists(path) && !path.ends_with(".exe") && baml.fs.exists(`${path}.exe`)) {
        path += ".exe";
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
        _ => {
            let message = match (error) {
                baml.toml.ParseError { message } | baml.errors.Io { message } => message,
                _ => error.to_string(),
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
        return "//" + resolved.slice(8, resolved.length()).replace_all("\\", "/");
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
    null
}

function selection(paths: WrapperInvocation) -> Selection {
    let active = selection_impl(paths) catch (error) {
        _ => {
            telemetry_event(paths.telemetry, "wrapper_resolution", fields = { "outcome": "failure" });
            throw error;
        }
    };
    let source = if (active.source == "$BAML_TOOLCHAIN") {
        "environment"
    } else if (active.source == "default canary") {
        "fallback"
    } else if (active.source.ends_with("baml.toml")) {
        "project"
    } else {
        "user"
    };
    telemetry_event(
        paths.telemetry,
        "wrapper_resolution",
        fields = { "outcome": "success", "source": source, "toolchain_kind": telemetry_kind(active.value) },
    );
    active
}

function selection_impl(paths: WrapperInvocation) -> Selection {
    let env = (baml.env.get("BAML_TOOLCHAIN") ?? "").trim();
    if (env != "") {
        validate_selector(env);
        return Selection { value: normalized(env, paths.directory), source: "$BAML_TOOLCHAIN" };
    }
    if let path: string = closest_manifest(start = paths.project) {
        let doc = document(path);
        if let toolchain: baml.toml.Table = table_at(doc, "toolchain", path) {
            for (let key in ["version", "channel", "path"]) {
                if let value: string = string_at(toolchain, key, `${path}: toolchain`) {
                    value = value.trim();
                    if (key == "version") {
                        version(value);
                    }
                    if (key == "channel") {
                        ensure(channel(value), `${path}: invalid toolchain.channel ${value}`);
                    }
                    return Selection {
                        value: if (key == "path") {
                            absolute(value, parent(path))
                        } else {
                            value
                        },
                        source: path,
                    };
                }
            }
        }
    }
    let config = document(config_path());
    if let defaults: baml.toml.Table = table_at(config, "default", config_path()) {
        if let value: string = string_at(defaults, "selector", config_path()) {
            if (value.trim() != "") {
                validate_selector(value.trim());
                return Selection {
                    value: normalized(value.trim(), paths.directory),
                    source: config_path(),
                };
            }
        }
    }
    Selection { value: "canary", source: "default canary" }
}

function validate_config(text: string) -> null {
    let doc = baml.toml.Table.parse(text);
    if let defaults: baml.toml.Table = table_at(doc, "default", config_path()) {
        if let value: string = string_at(defaults, "selector", config_path()) {
            validate_selector(value);
        }
    }
    if let update: baml.toml.Table = table_at(doc, "update", config_path()) {
        match (update.items.get("auto_check")) {
            null | bool => {},
            _ => {
                ensure(false, `${config_path()}: update.auto_check must be a boolean`);
            },
        }
    }
    null
}

function validate_project(text: string, path: string) -> null {
    let doc = baml.toml.Table.parse(text);
    baml.json.to<ProjectShape>(doc.to_json(), naming = baml.json.Naming.Aliased);
    // JSON renders TOML datetimes as strings. Check string-valued fields in
    // the original tree too, so a date cannot masquerade as a valid setting.
    for (let section in ["pack", "bridge", "boundary"]) {
        if let table: baml.toml.Table = table_at(doc, section, path) {
            validate_strings(
                table,
                if (section == "boundary") {
                    ["project", "api_url"]
                } else {
                    ["telemetry_environment"]
                },
                path,
            );
        }
    }
    if let dependencies: baml.toml.Table = table_at(doc, "dependencies", path) {
        for (let key in dependencies.items.keys()) {
            if let dependency: baml.toml.Table = table_at(dependencies, key, path) {
                validate_strings(dependency, ["path"], path);
            }
        }
    }
    if let generators: baml.toml.Table = table_at(doc, "generator", path) {
        for (let key in generators.items.keys()) {
            if let generator_options: baml.toml.Table = table_at(generators, key, path) {
                validate_strings(
                    generator_options,
                    ["output_type", "naming_convention", "output_dir", "sdk_import_path"],
                    path,
                );
            }
        }
    }
    if let scripts: baml.toml.Table = table_at(doc, "scripts", path) {
        for (let key in scripts.items.keys()) {
            match (scripts.items[key]) {
                string => {},
                let tokens: baml.toml.Item[] => {
                    validate_string_items(tokens, path);
                },
                _ => {
                    ensure(false, `${path}: scripts.${key} must be a string or string array`);
                },
            }
        }
    }
    if let test_options: baml.toml.Table = table_at(doc, "test", path) {
        validate_strings(test_options, ["default"], path);
        if let profiles: baml.toml.Table = table_at(test_options, "profiles", path) {
            for (let key in profiles.items.keys()) {
                if let profile: baml.toml.Table = table_at(profiles, key, path) {
                    match (profile.items.get("args")) {
                        null => {},
                        let tokens: baml.toml.Item[] => {
                            validate_string_items(tokens, path);
                        },
                        _ => {
                            ensure(false, `${path}: test profile args must be a string array`);
                        },
                    }
                }
            }
        }
    }
    if let package: baml.toml.Table = table_at(doc, "package", path) {
        string_at(package, "name", path);
        match (package.items.get("prelude")) {
            null | bool => {},
            _ => {
                ensure(false, `${path}: package.prelude must be a boolean`);
            },
        }
    }
    if let toolchain: baml.toml.Table = table_at(doc, "toolchain", path) {
        for (let key in ["version", "channel", "path"]) {
            if let value: string = string_at(toolchain, key, path) {
                if (key == "version") {
                    version(value);
                }
                if (key == "channel") {
                    ensure(channel(value), `${path}: invalid toolchain.channel`);
                }
                if (key == "path") {
                    ensure(value.trim() != "", `${path}: empty toolchain.path`);
                }
            }
        }
    }
    null
}

function validate_channel(table: baml.toml.Table, origin: string) -> string {
    let active = string_at(table, "active_version", origin) ?? "";
    version(active);
    ensure(
        string_at(table, "resolved_at", origin) != null
            && string_at(table, "manifest_path", origin) != null,
        `${origin}: incomplete channel state`,
    );
    string_at(table, "manifest_base_url", origin);
    active
}

function validate_state() -> null {
    validate_state_text(read_text(state_path()))
}

function validate_state_text(text: string) -> null {
    let doc = baml.toml.Table.parse(text);
    if let channels: baml.toml.Table = table_at(doc, "channels", state_path()) {
        for (let key in channels.items.keys()) {
            let value = table_at(channels, key, state_path());
            if let value: baml.toml.Table = value {
                validate_channel(value, `${state_path()}: channels.${key}`);
            }
        }
    }
    null
}

function recorded(name: string, override: string? = null) -> string? {
    let doc = document(state_path());
    if let channels: baml.toml.Table = table_at(doc, "channels", state_path()) {
        if let entry: baml.toml.Table = table_at(channels, name, state_path()) {
            let base = string_at(entry, "manifest_base_url", state_path());
            ensure(
                base == null || base == manifest_base(override = override),
                `${state_path()}: channel ${name} was selected from a different manifest source`,
            );
            let active = string_at(entry, "active_version", state_path()) ?? "";
            version(active);
            return active;
        }
    }
    null
}

function write_atomic(path: string, text: string) -> null {
    baml.fs.mkdir(parent(path), baml.fs.MkdirOptions { recursive: true });
    let tmp = `${path}.${baml.sys.pid()}.${int.random(0, int.max_value())}.tmp`;
    defer {
        if (baml.fs.exists(tmp)) {
            baml.fs.remove(tmp);
        }
    }
    baml.fs.write(tmp, text);
    baml.fs.rename(tmp, path);
    null
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

function channel_source_matches(name: string, override: string?) -> bool {
    let doc = document(state_path());
    if let channels: baml.toml.Table = table_at(doc, "channels", state_path()) {
        if let entry: baml.toml.Table = table_at(channels, name, state_path()) {
            let base = string_at(entry, "manifest_base_url", state_path());
            return base == null || base == manifest_base(override = override);
        }
    }
    false
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
    baml.fs.remove_dir_all(path) catch (_) {
        _ => {
            baml.fs.remove(path);
        }
    };
    null
}

class WrapperInvocation {
    telemetry: WrapperTelemetry?,
    args: string[],
    directory: string,
    project: string,
}

function invocation_paths(raw: string[]) -> WrapperInvocation {
    let directory: string? = null;
    let project: string? = null;
    let command = "";
    let args: string[] = [];
    let index = 0;
    while (index < raw.length()) {
        let arg = raw[index];
        if (arg == "--") {
            args = args.concat(raw.slice(index, raw.length()));
            break;
        }
        let flag = if (arg == "--directory" || arg.starts_with("--directory=")) {
            "--directory"
        } else if (command != "query" && (arg == "--project" || arg.starts_with("--project="))) {
            "--project"
        } else {
            ""
        };
        if (
            command == ""
                && [
                    "--output-preset",
                    "--color",
                    "--hyperlinks",
                    "--diagnostic-format",
                    "--agent-skill-check",
                ]
                    .includes(arg)
        ) {
            args.push(arg);
            index += 1;
            if (index < raw.length()) {
                args.push(raw[index]);
            }
            index += 1;
            continue;
        }
        if (flag != "") {
            let value = "";
            if (arg.starts_with(`${flag}=`)) {
                value = arg.slice(flag.length() + 1, arg.length());
            } else {
                index += 1;
                ensure(index < raw.length(), `${flag} requires a path`);
                value = raw[index];
                ensure(!value.starts_with("-"), `${flag} requires a path`);
            }
            ensure(value != "", `${flag} requires a path`);
            if (flag == "--directory") {
                ensure(directory == null, "--directory may only be supplied once");
                directory = value;
            } else {
                ensure(project == null, "--project may only be supplied once");
                project = value;
            }
        } else {
            if (command == "" && !arg.starts_with("-")) {
                command = arg;
            }
            args.push(arg);
        }
        index += 1;
    }
    let cwd = absolute(directory ?? baml.sys.cwd(), baml.sys.cwd());
    ensure(
        baml.fs.exists(cwd) && baml.fs.stat(cwd).is_dir,
        `directory does not exist or is not a directory: ${cwd}`,
    );
    let start = absolute(project ?? cwd, cwd);
    ensure(baml.fs.exists(start), `project path does not exist: ${start}`);
    if (!baml.fs.stat(start).is_dir) {
        start = parent(start);
    }
    WrapperInvocation {
        args: args,
        directory: canonical_directory(cwd),
        project: canonical_directory(start),
    }
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
}

function resolve_toolchain(
    paths: WrapperInvocation,
    install_missing: bool,
    override: string? = null,
) -> null {
    let active = selection(paths);
    let concrete: string? = null;
    let binary = active.value;
    let installed = true;
    if (is_path(active.value)) {
        verify_local(binary, active.source);
    } else {
        concrete
            = if (install_missing) {
                ensure_installed(active, override = override, telemetry = paths.telemetry);
                if (channel(active.value)) {
                    recorded(active.value, override = override)
                } else {
                    active.value
                }
            } else if (channel(active.value)) {
                recorded(active.value, override = override)
            } else {
                active.value
            };
        let value = concrete
            ?? fail("no active channel version is recorded; run baml toolchain resolve --install");
        binary = `${toolchains()}/${value}/bin/${cli_name()}`;
        installed = installation_present(value);
        if (installed) {
            verify_managed(value);
        }
    }
    baml.io.println(
        baml.json.stringify(
            ToolchainResolution {
                schema_version: 1,
                selector: active.value,
                version: concrete,
                source: active.source,
                wrapper_path: baml.sys.executable(),
                toolchain_path: absolute(binary, baml.sys.cwd()),
                managed: !is_path(active.value),
                installed: installed,
            },
        ),
    );
    null
}
