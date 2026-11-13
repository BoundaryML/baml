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
    format: OutputFormat,

    implements ManagementCommand {
        function preflight(self, invocation: WrapperInvocation) -> null {
            invocation.best_effort_install()
        }

        function execute(self, invocation: WrapperInvocation) -> null {
            invocation.status(self.install_missing, self.format)
        }
    }
}

class ListCommand {
    format: OutputFormat,

    implements ManagementCommand {
        function preflight(self, invocation: WrapperInvocation) -> null {
            invocation.best_effort_install()
        }

        function execute(self, invocation: WrapperInvocation) -> null {
            invocation.list_toolchains(self.format)
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
            baml.io.eprintln(`uninstalled BAML toolchain ${self.version}`);
            null
        }
    }
}

// The command a parsed `baml toolchain <name>` asks for.
function management_command(name: string, matches: root.clap.ArgMatches) -> ManagementCommand {
    match (name) {
        "use" => {
            UseCommand {
                selector: matches.get_one("selector") ?? "",
                global: matches.get_flag("global"),
            }
        },
        "install" => {
            InstallCommand {
                selector: matches.get_one("selector"),
                force: matches.get_flag("force"),
            }
        },
        "update" => UpdateCommand {},
        "status" => {
            StatusCommand {
                install_missing: matches.get_flag("install"),
                format: WrapperCli.output_format(matches),
            }
        },
        "list" => ListCommand { format: WrapperCli.output_format(matches) },
        "uninstall" => {
            UninstallCommand { version: ManagedVersion.new(matches.get_one("version") ?? "") }
        },
        _ => fail(`unknown toolchain command ${name}`),
    }
}
