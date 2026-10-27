function use_toolchain(
    value: string,
    local: bool,
    paths: WrapperInvocation,
    override: string? = null,
) -> null {
    let path = if (local) {
        config_path()
    } else {
        closest_manifest(start = paths.project) ?? ""
    };
    ensure(
        path != "",
        "no baml.toml found; run inside a BAML project or use: baml toolchain use --local <selector>",
    );
    let original = read_text(path);
    if (local) {
        validate_config(original);
    } else {
        validate_project(original, path);
    }
    let previous = previous_selector(original, local);
    let selected = normalized(
        value.trim(),
        if (local) {
            paths.directory
        } else {
            parent(path)
        },
    );
    validate_selector(selected);
    let updated = if (local) {
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
    if (local) {
        validate_config(updated);
    } else {
        validate_project(updated, path);
    }
    if (channel(selected) && !channel_source_matches(selected, override)) {
        install(selected, true, false, false, override = override);
    } else {
        ensure_installed(Selection { value: selected, source: path }, override = override);
    }
    write_atomic(path, updated);
    baml.io.println(
        `selected BAML toolchain ${describe_selector(selected)} in ${path} (previous: ${previous})`,
    );
    null
}

function update(paths: WrapperInvocation, override: string? = null) -> null {
    let active = selection(paths);
    if (!channel(active.value)) {
        baml.io.println(
            `active selector ${active.value} is ${if (is_path(active.value)) { "a local path" } else { "an exact version" }} and does not advance automatically`,
        );
        return null;
    }
    install(active.value, true, false, true, override = override);
    null
}

function status(paths: WrapperInvocation, override: string? = null) -> null {
    let active = selection(paths);
    baml.io.println(`selector: ${active.value} (${active.source})`);
    if (is_path(active.value)) {
        verify_local(active.value, active.source);
        return null;
    }
    if (channel(active.value)) {
        let concrete = recorded(active.value, override = override);
        let release = manifest(active.value, true, override = override);
        baml.io.println(`latest ${active.value}: ${release.version}`);
        baml.io.println(
            if (concrete == release.version) {
                "status: up to date"
            } else {
                "status: update available; run baml toolchain update"
            },
        );
    } else {
        for (let name in ["canary", "nightly"]) {
            let release = manifest(name, true, override = override);
            baml.io.println(`latest ${name}: ${release.version}`);
        }
        baml.io.println("status: exact versions do not advance automatically");
    }
    null
}

function list_toolchains(paths: WrapperInvocation) -> null {
    let config = document(config_path());
    let defaults = "canary";
    if let table: baml.toml.Table = table_at(config, "default", config_path()) {
        defaults = string_at(table, "selector", config_path()) ?? "canary";
    }
    let active = selection(paths);
    baml.io.println(`default selector: ${defaults}`);
    baml.io.println(`active selector: ${active.value} (${active.source})`);
    let state = document(state_path());
    if let channels: baml.toml.Table = table_at(state, "channels", state_path()) {
        for (let name in channels.items.keys().sort()) {
            if let entry: baml.toml.Table = table_at(channels, name, state_path()) {
                let concrete = string_at(entry, "active_version", state_path()) ?? "";
                version(concrete);
                baml.io.println(`${name}: ${concrete}`);
            }
        }
    }
    let installed: string[] = [];
    if (baml.fs.exists(toolchains())) {
        for (let entry in baml.fs.read_dir(toolchains())) {
            if (entry.is_dir && !entry.name.starts_with(".")) {
                installed.push(entry.name);
            }
        }
    }
    if (installed.length() == 0) {
        baml.io.println("installed toolchains: (none)");
    } else {
        baml.io.println("installed toolchains:");
        for (let value in installed.sort()) {
            baml.io.println(`  ${value}`);
        }
    }
    baml.io.println("Remote versions were not checked.");
    null
}

function print_version(paths: WrapperInvocation) -> null {
    baml.io.println(`baml wrapper ${wrapper_version()}`);
    let active = selection(paths);
    if (is_path(active.value)) {
        verify_local(active.value, active.source);
        let output = baml.sys.exec(active.value, ["--version"], baml.sys.ProcessOptions { timeout_ms: 3000 });
        baml.io.println(
            `baml toolchain ${output.stdout.to_string().trim()} (local: ${active.value})`,
        );
    } else {
        let concrete = if (channel(active.value)) {
            recorded(active.value) ?? "not installed"
        } else {
            active.value
        };
        baml.io.println(`baml toolchain ${concrete} (${active.value}, ${active.source})`);
    }
    null
}

function help(command: string = "") -> null {
    let usage = match (command) {
        "use" => "use [--local] <selector>",
        "install" => "install [<selector>] [--force]",
        "resolve" => "resolve [--install] [--json]",
        "update" => "update",
        "status" => "status",
        "list" => "list",
        "uninstall" => "uninstall <version>",
        _ => "",
    };
    if (usage != "") {
        baml.io.println(`Usage: baml toolchain ${usage}`);
        baml.io.println("Options: --manifest-base-url <URL>, -h, --help");
        if (command == "resolve") {
            baml.io.println(
                "Emit the active selector, concrete version, source, paths and installation state as JSON. --install installs an absent managed toolchain first.",
            );
        }
        return null;
    }
    baml.io.println(
        "BAML toolchain management\n\nCommands:\n  use [--local] <selector>  Install if needed and select in the closest baml.toml\n                           --local selects in $BAML_HOME/config.toml instead\n  install [<selector>] [--force]\n  resolve [--install] [--json]  Resolve active selection as JSON\n  update\n  status\n  list\n  uninstall <version>\n\nSelectors: canary, nightly, exact release version, or a local baml-cli path\nWrapper updates: baml self-update",
    );
    null
}

function management(raw_args: string[], paths: WrapperInvocation) -> null {
    let args: string[] = [];
    let override: string? = null;
    let cursor = 0;
    while (cursor < raw_args.length()) {
        let arg = raw_args[cursor];
        if (arg == "--manifest-base-url" || arg.starts_with("--manifest-base-url=")) {
            ensure(override == null, "--manifest-base-url may only be supplied once");
            if (arg == "--manifest-base-url") {
                cursor += 1;
                ensure(cursor < raw_args.length(), "--manifest-base-url requires a value");
                override = raw_args[cursor];
            } else {
                override = arg.slice(20, arg.length());
            }
        } else {
            args.push(arg);
        }
        cursor += 1;
    }
    if (
        args.some((arg) -> bool {
            arg == "--help" || arg == "-h"
        })
    ) {
        help(
            command = if (args.length() > 0) {
                args[0]
            } else {
                ""
            },
        );
        return null;
    }
    if (args.length() == 0 || args == ["--help"] || args == ["-h"] || args == ["help"]) {
        help();
        return null;
    }
    let command = args[0];
    let selector: string? = null;
    let local = false;
    let force = false;
    let install_missing = false;
    let json = false;
    let index = 1;
    while (index < args.length()) {
        let arg = args[index];
        if (arg == "--local" && command == "use" && !local) {
            local = true;
        } else if (arg == "--force" && command == "install" && !force) {
            force = true;
        } else if (arg == "--install" && command == "resolve" && !install_missing) {
            install_missing = true;
        } else if (arg == "--json" && command == "resolve" && !json) {
            json = true;
        } else if (
            !arg.starts_with("-")
                && selector == null
                && (command == "use" || command == "install" || command == "uninstall")
        ) {
            selector = arg;
        } else {
            ensure(false, `unexpected argument ${arg} for toolchain ${command}`);
        }
        index += 1;
    }
    match (command) {
        "use" => {
            use_toolchain(
                selector ?? fail("toolchain use requires a selector"),
                local,
                paths,
                override = override,
            );
        },
        "install" => {
            let requested = selector ?? resolved_install_selector(paths, override = override);
            install(requested, selector == null && channel(requested), force, false, override = override);
        },
        "resolve" => {
            resolve_toolchain(paths, install_missing, override = override);
        },
        "update" => {
            update(paths, override = override);
        },
        "status" => {
            status(paths, override = override);
        },
        "list" => {
            list_toolchains(paths);
        },
        "uninstall" => {
            let value = selector ?? fail("toolchain uninstall requires a concrete version");
            version(value);
            let path = `${toolchains()}/${value}`;
            ensure(baml.fs.exists(path), `toolchain ${value} is not installed`);
            baml.fs.remove_dir_all(path);
            baml.io.println(`uninstalled BAML toolchain ${value}`);
        },
        _ => {
            ensure(false, `unknown toolchain command ${command}`);
        },
    }
    null
}

function fail(message: string) -> never throws baml.errors.InvalidArgument {
    throw baml.errors.InvalidArgument { message: message }
}

/// Parameterless packed entry point owns its complete argv. The pack host adds
/// the entry identifier at argv[1]; forward every subsequent token unchanged.
function main() -> void {
    dispatch() catch (error) {
        _ => {
            let message = match (error) {
                baml.errors.InvalidArgument { message }
                    | baml.errors.Io { message }
                    | baml.errors.ParseError { message }
                    | baml.errors.Timeout { message } =>
                {
                    message
                },
                _ => error.to_string(),
            };
            baml.io.eprintln(`error: ${message}`);
            baml.sys.exit(1);
        },
    };
    baml.sys.exit(0);
}

function dispatch() -> null {
    let argv = baml.sys.argv();
    let forwarded = argv.slice(2, argv.length());
    let paths = invocation_paths(forwarded);
    let args = paths.args;
    if (args.length() > 0) {
        if (args[0] == "--replace") {
            replace_wrapper(args.slice(1, args.length()));
            return null;
        }
        if (args[0] == "--version" || args[0] == "-V") {
            print_version(paths);
            return null;
        }
        if (args[0] == "toolchain") {
            management(args.slice(1, args.length()), paths);
            return null;
        }
        if (args[0] == "self-update") {
            if (args == ["self-update", "--help"] || args == ["self-update", "-h"]) {
                baml.io.println(
                    "baml self-update: update the curl-installed wrapper without changing toolchains",
                );
                return null;
            }
            ensure(args.length() == 1, "unexpected self-update arguments");
            self_update();
            return null;
        }
    }
    if (args.length() > 0 && (args[0] == "update" || args[0] == "upgrade")) {
        fail(
            "use `baml toolchain update` to update the selected language channel.\nUse `baml self-update` to update the wrapper, or `baml toolchain status` to check available toolchain updates.",
        );
    }
    let active = selection(paths);
    let binary = ensure_installed(active);
    let warned = warn_if_outdated(active);
    let refresh = spawn { refresh_channel(active) };
    let result = baml.sys.run(binary, forwarded, { "BAML_CLI_ALLOW_DIRECT": "1" });
    await refresh;
    if (!warned) {
        warn_if_outdated(active);
    }
    baml.sys.exit(result.exit_code)
}

function resolved_install_selector(paths: WrapperInvocation, override: string? = null) -> string {
    let active = selection(paths);
    ensure(!is_path(active.value), "active toolchain is a local path; there is nothing to install");
    if (channel(active.value)) {
        // Keep an already-resolved channel on its recorded concrete version.
        return recorded(active.value, override = override) ?? active.value;
    }
    active.value
}

function describe_selector(value: string) -> string {
    if (channel(value)) {
        let active = recorded(value) catch (_) {
            _ => null
        };
        if let active: string = active {
            return `${value} (${active})`;
        }
    }
    value
}

function previous_selector(original: string, local: bool) -> string {
    let doc = baml.toml.Table.parse(original);
    if (local) {
        if let defaults: baml.toml.Table = table_at(doc, "default", config_path()) {
            return describe_selector(string_at(defaults, "selector", config_path()) ?? "canary");
        }
        return describe_selector("canary");
    }
    if let toolchain: baml.toml.Table = table_at(doc, "toolchain", "baml.toml") {
        for (let key in ["version", "channel", "path"]) {
            if let value: string = string_at(toolchain, key, "baml.toml") {
                return describe_selector(value);
            }
        }
    }
    "not set"
}
