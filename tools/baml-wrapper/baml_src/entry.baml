function use_toolchain(
    value: string,
    local: bool,
    paths: WrapperInvocation,
    override: string? = null,
) -> null {
    use_toolchain_impl(value, local, paths, override = override) catch (error) {
        _ => {
            telemetry_event(
                paths.telemetry,
                "wrapper_selection",
                fields = {
                    "outcome": "failure",
                    "scope": if (local) {
                        "user"
                    } else {
                        "project"
                    },
                    "toolchain_kind": telemetry_kind(value),
                },
            );
            throw error;
        }
    };
    telemetry_event(
        paths.telemetry,
        "wrapper_selection",
        fields = {
            "outcome": "success",
            "scope": if (local) {
                "user"
            } else {
                "project"
            },
            "toolchain_kind": telemetry_kind(value),
        },
    );
    null
}

function use_toolchain_impl(
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
    if (channel(selected)) {
        let current = recorded(selected, override = override);
        if (
            current == null
                || !channel_source_matches(selected, override)
                || !installation_present(current ?? "")
        ) {
            validate_state();
        }
    }
    best_effort_install(paths);
    if (channel(selected) && !channel_source_matches(selected, override)) {
        install(selected, true, false, false, override = override, telemetry = paths.telemetry);
    } else {
        ensure_installed(
            Selection { value: selected, source: path },
            override = override,
            telemetry = paths.telemetry,
        );
    }
    write_atomic(path, updated);
    baml.io.println(
        `selected BAML toolchain ${describe_selector(selected)} in ${path} (previous: ${previous})`,
    );
    null
}

function update(paths: WrapperInvocation, override: string? = null) -> null {
    update_impl(paths, override = override) catch (error) {
        _ => {
            telemetry_event(paths.telemetry, "wrapper_update", fields = { "outcome": "failure" });
            throw error;
        }
    };
    telemetry_event(paths.telemetry, "wrapper_update", fields = { "outcome": "success" });
    null
}

function update_impl(paths: WrapperInvocation, override: string? = null) -> null {
    let active = selection(paths);
    if (!channel(active.value)) {
        baml.io.println(
            `active selector ${active.value} is ${if (is_path(active.value)) { "a local path" } else { "an exact version" }} and does not advance automatically`,
        );
        return null;
    }
    install(active.value, true, false, true, override = override, telemetry = paths.telemetry);
    null
}

function status(paths: WrapperInvocation, install_missing: bool, json: bool) -> null {
    let result = resolve_toolchain(paths, install_missing);
    if (result.managed) {
        let channels = if (channel(result.selector)) {
            [result.selector]
        } else {
            ["canary", "nightly"]
        };
        for (let name in channels) {
            let release = manifest(name, true);
            result.available_releases[name] = release.version;
        }
    }
    if (json) {
        baml.io.println(baml.json.stringify(result));
        return null;
    }
    baml.io.println(`active selector: ${result.selector}`);
    baml.io.println(`source: ${result.source}`);
    baml.io.println(`installed: ${result.installed}`);
    if (!result.managed) {
        baml.io.println("status: local toolchain binary, not managed by the wrapper");
        baml.io.println("Remote versions were not checked.");
        return null;
    }
    baml.io.println(`selected version: ${result.version ?? "unknown"}`);
    for (let name in result.available_releases.keys().sort()) {
        baml.io.println(`latest ${name}: ${result.available_releases[name]}`);
    }
    if (channel(result.selector)) {
        baml.io.println(
            if (result.available_releases[result.selector] == result.version) {
                "status: up to date"
            } else {
                "status: update available; run: baml toolchain update"
            },
        );
    } else {
        baml.io.println("status: exact versions do not advance automatically");
    }
    null
}

function best_effort_install(paths: WrapperInvocation) -> null {
    ensure_installed(selection(paths), telemetry = paths.telemetry) catch (error) {
        _ => {
            baml.io.eprintln(
                `warning: could not ensure active baml-cli is installed: ${error_message(error)}`,
            );
            ""
        }
    };
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

function print_version(paths: WrapperInvocation, flag: string) -> null {
    baml.io.println(`baml wrapper ${wrapper_version()}`);
    let binary = ensure_installed(selection(paths), telemetry = paths.telemetry);
    telemetry_finish(paths.telemetry);
    let result = baml.sys.run(binary, [flag], { "BAML_CLI_ALLOW_DIRECT": "1" });
    baml.sys.exit(result.exit_code)
}

function root_help() -> null {
    baml.io.println(
        "BAML\n\nUsage: baml [--project <path>] <command> [args...]\n\nWrapper commands:\n  toolchain     Manage language toolchains\n  self-update   Update the wrapper executable\n\nCLI commands (provided by the selected toolchain):\n  init, new, check, fmt, generate, test, run, pack, describe, query,\n  auth, feedback, ide, agent, playground, lsp, telemetry, clean, update, upgrade\n\nOptions:\n  --project <path>  Select a filesystem project\n  -h, --help       Show help\n  -V, --version    Report wrapper and selected CLI versions\n\nUse baml help <command> for command help.",
    );
    null
}

function help(command: string = "") -> null {
    let usage = match (command) {
        "use" => "use [--local] <selector>",
        "install" => "install [<selector>] [--force]",
        "update" => "update",
        "status" => "status [--install] [--json]",
        "list" => "list",
        "uninstall" => "uninstall <version>",
        _ => "",
    };
    if (usage != "") {
        baml.io.println(`Usage: baml toolchain ${usage}`);
        baml.io.println("Options: -h, --help");
        if (command == "status") {
            baml.io.println(
                "Emit the active selector, concrete version, source, paths and installation state as JSON. --install installs an absent managed toolchain first.",
            );
        }
        return null;
    }
    baml.io.println(
        "BAML toolchain management\n\nCommands:\n  use [--local] <selector>  Install if needed and select in the closest baml.toml\n                           --local selects in $BAML_HOME/config.toml instead\n  install [<selector>] [--force]\n  update\n  status [--install] [--json]\n  list\n  uninstall <version>\n\nSelectors: canary, nightly, exact release version, or a local baml-cli path\nWrapper updates: baml self-update",
    );
    null
}

function management(args: string[], paths: WrapperInvocation) -> null {
    let override: string? = null;
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
    if (args[0] == "help") {
        help(command = args.at(1) ?? "");
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
        } else if (arg == "--install" && command == "status" && !install_missing) {
            install_missing = true;
        } else if (arg == "--json" && command == "status" && !json) {
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
    ensure(
        ["use", "install", "status", "update", "list", "uninstall"].includes(command),
        `unknown toolchain command ${command}`,
    );
    if (command == "use") {
        ensure(selector != null, "toolchain use requires a selector");
    } else {
        if (command == "install") {
            if let requested: string = selector {
                validate_selector(requested);
            }
        }
        if (command == "uninstall") {
            version(selector ?? fail("toolchain uninstall requires a concrete version"));
        }
        if (command == "update" && channel(selection(paths).value)) {
            validate_state();
        }
        best_effort_install(paths);
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
            if (channel(requested)) {
                requested = recorded(requested, override = override) ?? requested;
            }
            install(
                requested,
                selector == null && channel(requested),
                force,
                false,
                override = override,
                telemetry = paths.telemetry,
            );
        },
        "update" => {
            update(paths, override = override);
        },
        "status" => {
            status(paths, install_missing, json);
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
function error_message(error: unknown) -> string {
    match (error) {
        baml.errors.InvalidArgument { message }
            | baml.errors.Io { message }
            | baml.errors.ParseError { message }
            | baml.toml.ParseError { message }
            | baml.json.DecodeError { message }
            | baml.json.SerializationError { message }
            | baml.errors.Timeout { message } =>
        {
            message
        },
        _ => error.to_string(),
    }
}

function main() -> void {
    dispatch() catch (error) {
        _ => {
            baml.io.eprintln(`error: ${error_message(error)}`);
            baml.sys.exit(1);
        },
    };
    baml.sys.exit(0);
}

function dispatch() -> null {
    let argv = baml.sys.argv();
    let forwarded = argv.slice(2, argv.length());
    if (forwarded == ["__flush-wrapper-telemetry"]) {
        telemetry_drain();
        return null;
    }
    if (forwarded.at(0) == "--replace") {
        replace_wrapper(forwarded.slice(1, forwarded.length()));
        return null;
    }
    let paths = invocation_paths(forwarded);
    let args = paths.args;
    paths.telemetry = telemetry_begin(args, paths.directory);
    defer { telemetry_finish(paths.telemetry); }
    if (args.length() == 0 || args == ["--help"] || args == ["-h"] || args == ["help"]) {
        root_help();
        return null;
    }
    if (args[0] == "help" && args.at(1) == "toolchain") {
        management(args.slice(2, args.length()).concat(["--help"]), paths);
        return null;
    }
    if (args[0] == "help" && args.at(1) == "self-update") {
        ensure(args.length() == 2, "usage: baml help self-update");
        baml.io.println(
            "baml self-update: update the wrapper executable independently of language toolchains; use baml toolchain status to check language updates",
        );
        return null;
    }
    if (args.length() > 0) {
        if (args[0] == "--version" || args[0] == "-V") {
            print_version(paths, args[0]);
            return null;
        }
        if (args[0] == "toolchain") {
            management(args.slice(1, args.length()), paths);
            return null;
        }
        if (args[0] == "self-update") {
            if (args == ["self-update", "--help"] || args == ["self-update", "-h"]) {
                baml.io.println(
                    "baml self-update: update the wrapper executable independently of language toolchains; use baml toolchain status to check language updates",
                );
                return null;
            }
            ensure(args.length() == 1, "unexpected self-update arguments");
            self_update(telemetry = paths.telemetry);
            return null;
        }
    }
    let active = selection(paths);
    let binary = ensure_installed(active, telemetry = paths.telemetry);
    let warned = warn_if_outdated(active);
    let refresh = spawn { refresh_channel(active) };
    telemetry_finish(paths.telemetry);
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
