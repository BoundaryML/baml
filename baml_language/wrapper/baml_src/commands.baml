type ManagementName = "use" | "install" | "update" | "status" | "list" | "uninstall";

type ManagementFlag = "--global" | "--force" | "--install";

const OUTPUT_FORMAT_FLAG = "--output-format";

class UnexpectedManagementArgument {
    command: string,
    argument: string,

    implements baml.ToString {
        function to_string(self) -> string {
            `unexpected argument ${self.argument} for toolchain ${self.command}`
        }
    }
}

class UnknownManagementCommand {
    command: string,

    implements baml.ToString {
        function to_string(self) -> string {
            `unknown toolchain command ${self.command}`
        }
    }
}

interface ManagementCommand {
    function preflight(self, invocation: WrapperInvocation) -> null throws WrapperFailure;
    function execute(self, invocation: WrapperInvocation) -> null throws WrapperFailure;
}

class UseCommand {
    selector: string,
    global: bool,

    implements ManagementCommand {
        // Selection performs its write preflight before the best-effort install.
        function preflight(self, invocation: WrapperInvocation) -> null {
            null
        }
        function execute(self, invocation: WrapperInvocation) -> null {
            invocation.use_toolchain(self.selector, self.global)
        }
    }
}

class InstallCommand {
    selector: string?,
    force: bool,

    implements ManagementCommand {
        function preflight(self, invocation: WrapperInvocation) -> null {
            if let requested: string = self.selector {
                validate_selector(requested);
            }
            invocation.best_effort_install()
        }
        function execute(self, invocation: WrapperInvocation) -> null {
            let requested = self.selector ?? invocation.install_selector();
            if (channel(requested)) {
                requested = ChannelState.new().recorded(requested) ?? requested;
            }
            ReleaseStore.new().install(
                requested,
                self.selector == null && channel(requested),
                self.force,
                false,
                telemetry = invocation.telemetry,
            );
            null
        }
    }
}

class UpdateCommand {
    implements ManagementCommand {
        function preflight(self, invocation: WrapperInvocation) -> null {
            if (invocation.selection().selector is ChannelSelector) {
                ChannelState.new().validate();
            }
            invocation.best_effort_install()
        }
        function execute(self, invocation: WrapperInvocation) -> null {
            invocation.update()
        }
    }
}

class StatusCommand {
    install_missing: bool,
    json: bool,

    implements ManagementCommand {
        function preflight(self, invocation: WrapperInvocation) -> null {
            invocation.best_effort_install()
        }

        function execute(self, invocation: WrapperInvocation) -> null {
            invocation.status(self.install_missing, self.json)
        }
    }
}

class ListCommand {
    implements ManagementCommand {
        function preflight(self, invocation: WrapperInvocation) -> null {
            invocation.best_effort_install()
        }

        function execute(self, invocation: WrapperInvocation) -> null {
            invocation.list_toolchains()
        }
    }
}

class UninstallCommand {
    version: ManagedVersion,

    implements ManagementCommand {
        function preflight(self, invocation: WrapperInvocation) -> null {
            invocation.best_effort_install()
        }

        function execute(self, invocation: WrapperInvocation) -> null {
            let path = `${toolchains()}/${self.version}`;
            ensure(baml.fs.exists(path), `toolchain ${self.version} is not installed`);
            baml.fs.remove_dir_all(path);
            baml.io.println(`uninstalled BAML toolchain ${self.version}`);
            null
        }
    }
}

// Parse the wire syntax once; command classes represent only applicable options.
function management_command(args: string[]) -> ManagementCommand {
    let raw = args[0];
    let name: ManagementName = match (raw) {
        "use" | "install" | "update" | "status" | "list" | "uninstall" => raw,
        _ => throw UnknownManagementCommand { command: raw },
    };
    let allowed: ManagementFlag[] = match (name) {
        "use" => ["--global"],
        "install" => ["--force"],
        "status" => ["--install"],
        "update" | "list" | "uninstall" => [],
    };
    let options = args.slice(1, args.length());
    let flags: string[] = [];
    let positional: string[] = [];
    let output_format: string? = null;
    let index = 0;
    while (index < options.length()) {
        let arg = options[index];
        index += 1;
        if (
            name == "status"
                && (arg == OUTPUT_FORMAT_FLAG || arg.starts_with(`${OUTPUT_FORMAT_FLAG}=`))
        ) {
            if (output_format != null) {
                throw UnexpectedManagementArgument { command: name, argument: OUTPUT_FORMAT_FLAG }
            }
            if (arg == OUTPUT_FORMAT_FLAG) {
                ensure(index < options.length(), `${OUTPUT_FORMAT_FLAG} requires text or json`);
                output_format = options[index];
                index += 1;
            } else {
                output_format = arg.slice(OUTPUT_FORMAT_FLAG.length() + 1, arg.length());
            }
        } else if (arg.starts_with("-")) {
            flags.push(
                if (name == "use" && arg == "-g") {
                    "--global"
                } else {
                    arg
                },
            );
        } else {
            positional.push(arg);
        }
    }
    for (let flag in flags) {
        if (
            !allowed.some((option) -> bool {
                option == flag
            })
                || flags
                    .filter((arg) -> bool {
                        arg == flag
                    })
                    .length()
                    > 1
        ) {
            throw UnexpectedManagementArgument { command: name, argument: flag }
        }
    }
    let accepts_selector = match (name) {
        "use" | "install" | "uninstall" => true,
        "update" | "status" | "list" => false,
    };
    let positional_limit: int = if (accepts_selector) {
        1
    } else {
        0
    };
    if (positional.length() > positional_limit) {
        throw UnexpectedManagementArgument { command: name, argument: positional[positional.length() - 1] }
    }
    let selector = positional.at(0);
    match (name) {
        "use" => {
            UseCommand {
                selector: selector ?? fail("toolchain use requires a selector"),
                global: flags.includes("--global"),
            }
        },
        "install" => InstallCommand { selector: selector, force: flags.includes("--force") },
        "update" => UpdateCommand {},
        "status" => {
            StatusCommand {
                install_missing: flags.includes("--install"),
                json: match (output_format ?? "text") {
                    "text" => false,
                    "json" => true,
                    _ => fail(`${OUTPUT_FORMAT_FLAG} must be text or json`),
                },
            }
        },
        "list" => ListCommand {},
        "uninstall" => {
            UninstallCommand {
                version: ManagedVersion.new(
                    selector ?? fail("toolchain uninstall requires a concrete version"),
                ),
            }
        },
    }
}
