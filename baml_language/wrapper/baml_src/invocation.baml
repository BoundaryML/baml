class WrapperInvocation {
    telemetry: WrapperTelemetry?,
    args: string[],
    directory: string,
    project: string,

    function selection(self) -> Selection {
        let active = self.selection_impl() catch (error) {
            _ => {
                telemetry_event(self.telemetry, "wrapper_resolution", fields = { "outcome": "failure" });
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
            self.telemetry,
            "wrapper_resolution",
            fields = { "outcome": "success", "source": source, "toolchain_kind": active.selector.kind() },
        );
        active
    }

    function selection_impl(self) -> Selection {
        let env = (baml.env.get("BAML_TOOLCHAIN") ?? "").trim();
        if (env) {
            validate_selector(env);
            return Selection.new(normalized(env, self.directory), "$BAML_TOOLCHAIN");
        }
        if let path: string = closest_manifest(start = self.project) {
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
                        return Selection.new(
                            if (key == "path") {
                                absolute(value, parent(path))
                            } else {
                                value
                            },
                            path,
                        );
                    }
                }
            }
        }
        let config = document(config_path());
        if let defaults: baml.toml.Table = table_at(config, "default", config_path()) {
            if let value: string = string_at(defaults, "selector", config_path()) {
                if (value.trim()) {
                    validate_selector(value.trim());
                    return Selection.new(normalized(value.trim(), self.directory), config_path());
                }
            }
        }
        Selection.new("canary", "default canary")
    }

    function resolve(self, install_missing: bool) -> ToolchainResolution {
        let active = self.selection();
        let concrete: string? = null;
        let binary = active.selector.to_string();
        let installed = true;
        if (is_path(active.selector.to_string())) {
            verify_local(binary, active.source);
        } else {
            concrete
                = if (install_missing) {
                    active.ensure_installed(telemetry = self.telemetry);
                    if (channel(active.selector.to_string())) {
                        ChannelState.new().recorded(active.selector.to_string())
                    } else {
                        active.selector.to_string()
                    }
                } else if (channel(active.selector.to_string())) {
                    ChannelState.new().recorded(active.selector.to_string())
                } else {
                    active.selector.to_string()
                };
            let value = concrete
                ?? fail("no active channel version is recorded; run baml toolchain status --install");
            binary = `${toolchains()}/${value}/bin/${cli_name()}`;
            installed = installation_present(value);
            if (installed) {
                verify_managed(value);
            }
        }
        ToolchainResolution {
            schema_version: 1,
            selector: active.selector.to_string(),
            version: concrete,
            source: active.source,
            wrapper_path: baml.sys.current_exe(),
            toolchain_path: absolute(binary, baml.sys.current_dir()),
            managed: !is_path(active.selector.to_string()),
            installed: installed,
            available_releases: {},
        }
    }

    function use_toolchain(self, value: string, global: bool) -> null {
        self.use_toolchain_impl(value, global) catch (error) {
            _ => {
                telemetry_event(
                    self.telemetry,
                    "wrapper_selection",
                    fields = {
                        "outcome": "failure",
                        "scope": if (global) {
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
            self.telemetry,
            "wrapper_selection",
            fields = {
                "outcome": "success",
                "scope": if (global) {
                    "user"
                } else {
                    "project"
                },
                "toolchain_kind": telemetry_kind(value),
            },
        );
        null
    }

    function use_toolchain_impl(self, value: string, global: bool) -> null {
        let path = if (global) {
            config_path()
        } else {
            closest_manifest(start = self.project)
                ?? throw ProjectNotFound { directory: self.project }
        };
        let original = read_text(path);
        if (global) {
            UserConfiguration.new(original);
        } else {
            ProjectConfiguration.new(path, original);
        }
        let previous = previous_selector(original, global);
        let selected = normalized(
            value.trim(),
            if (global) {
                self.directory
            } else {
                parent(path)
            },
        );
        validate_selector(selected);
        selection_document(path, original, selected, global);
        if (channel(selected)) {
            if (!ChannelState.new().source_matches(selected)) {
                ChannelState.new().validate();
            } else {
                let current = ChannelState.new().recorded(selected);
                if (current == null || !installation_present(current ?? "")) {
                    ChannelState.new().validate();
                }
            }
        }
        self.best_effort_install();
        if (channel(selected) && !ChannelState.new().source_matches(selected)) {
            ReleaseStore.new().install(selected, true, false, false, telemetry = self.telemetry);
        } else {
            Selection.new(selected, path).ensure_installed(telemetry = self.telemetry);
        }
        // Reapply the selection to the latest validated document after downloads.
        let updated = selection_document(path, read_text(path), selected, global);
        write_atomic(path, updated);
        baml.io.println(
            `selected BAML toolchain ${describe_selector(selected)} in ${path} (previous: ${previous})`,
        );
        null
    }

    function update(self) -> null {
        self.update_impl() catch (error) {
            _ => {
                telemetry_event(self.telemetry, "wrapper_update", fields = { "outcome": "failure" });
                throw error;
            }
        };
        telemetry_event(self.telemetry, "wrapper_update", fields = { "outcome": "success" });
        null
    }

    function update_impl(self) -> null {
        self.selection().selector.update(self.telemetry);
        null
    }

    function status(self, install_missing: bool, json: bool) -> null {
        let result = self.resolve(install_missing);
        if (result.managed) {
            let channels = if (channel(result.selector)) {
                [result.selector]
            } else {
                ["canary", "nightly"]
            };
            for (let name in channels) {
                let release = ReleaseStore.new().manifest(name, true);
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

    function best_effort_install(self) -> null {
        self.selection().ensure_installed(telemetry = self.telemetry) catch (error) {
            WrapperFailure => {
                baml.io.eprintln(
                    `warning: could not ensure active baml-cli is installed: ${error_message(error)}`,
                );
                null
            }
        };
        null
    }

    function list_toolchains(self) -> null {
        let config = document(config_path());
        let defaults = "canary";
        if let table: baml.toml.Table = table_at(config, "default", config_path()) {
            defaults = string_at(table, "selector", config_path()) ?? "canary";
        }
        let active = self.selection();
        baml.io.println(`default selector: ${defaults}`);
        baml.io.println(`active selector: ${active.selector.to_string()} (${active.source})`);
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
        let installed: string[] = if (baml.fs.exists(toolchains())) {
            baml.fs.read_dir(toolchains())
                .filter((entry) -> bool {
                    entry.is_dir && !entry.name.starts_with(".")
                })
                .map((entry) -> string {
                    entry.name
                })
        } else {
            []
        };
        if (!installed) {
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

    function print_version(self, flag: string) -> null {
        baml.io.println(`baml wrapper ${wrapper_version()}`);
        let binary = self.selection().ensure_installed(telemetry = self.telemetry);
        WrapperTelemetry.finish(self.telemetry);
        let result = baml.sys.run(binary, args = [flag], env = { "BAML_CLI_ALLOW_DIRECT": "1" });
        baml.sys.exit(result.exit_code)
    }

    function management(self, args: string[]) -> null {
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
        let command = management_command(args);
        command.preflight(self, $trace = trace.span(inputs = false, output = false, error = false));
        command.execute(self, $trace = trace.span(inputs = false, output = false, error = false))
    }

    function install_selector(self) -> string {
        let active = self.selection();
        ensure(
            !is_path(active.selector.to_string()),
            "active toolchain is a local path; there is nothing to install",
        );
        if (channel(active.selector.to_string())) {
            // Keep an already-resolved channel on its recorded concrete version.
            return ChannelState.new().recorded(active.selector.to_string()) ?? active.selector.to_string();
        }
        active.selector.to_string()
    }

    function parse(raw: string[]) -> WrapperInvocation {
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
            let flag = if (command != "query" && (arg == "--project" || arg.starts_with("--project="))) {
                "--project"
            } else {
                ""
            };
            if (
                !command
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
            if (flag) {
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
                ensure(project == null, "--project may only be supplied once");
                project = value;
            } else {
                if (!command && !arg.starts_with("-")) {
                    command = arg;
                }
                args.push(arg);
            }
            index += 1;
        }
        let cwd = baml.sys.current_dir();
        ensure(
            baml.fs.exists(cwd) && baml.fs.metadata(cwd).is_dir,
            `directory does not exist or is not a directory: ${cwd}`,
        );
        let start = absolute(project ?? cwd, cwd);
        ensure(baml.fs.exists(start), `project path does not exist: ${start}`);
        if (!baml.fs.metadata(start).is_dir) {
            start = parent(start);
        }
        WrapperInvocation {
            args: args,
            directory: canonical_directory(cwd),
            project: canonical_directory(start),
        }
    }
}
