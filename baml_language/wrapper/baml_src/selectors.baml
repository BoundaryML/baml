type ChannelName = "canary" | "nightly";

type ToolchainKind = "version" | "channel" | "path";

class InvalidToolchainVersion {
    value: string,

    implements baml.ToString {
        function to_string(self) -> string {
            `invalid managed toolchain version ${self.value}`
        }
    }
}

class ManagedVersion {
    value: string,

    function new(value: string) -> Self throws InvalidToolchainVersion {
        // SemVer, including prerelease identifiers and build metadata, e.g.
        // 0.22.3-nightly.20261021.a+build.001. Numeric identifiers cannot have leading zeroes.
        let number = "(0|[1-9][0-9]*)";
        let identifier = "(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)";
        let pattern = `^${number}\\.${number}\\.${number}(-${identifier}(\\.${identifier})*)?(\\+[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?$`;
        if (
            !(
                baml.regex.new(pattern) catch (error) {
                    baml.regex.Error => baml.sys.panic(`invalid built-in SemVer pattern: ${error}`),
                }
            )
                .has_match(value)
        ) {
            throw InvalidToolchainVersion { value: value }
        }
        let core = value.split("-")[0].split("+")[0];
        if (
            !core.split(".").every((component) -> bool {
                component.length() <= SEMVER_COMPONENT_MAX_DIGITS
                    && (
                        bigint.parse(component) catch (error) {
                            baml.errors.ParseError => throw InvalidToolchainVersion { value: value },
                        }
                    )
                        <= SEMVER_COMPONENT_MAX
            })
        ) {
            throw InvalidToolchainVersion { value: value }
        }
        ManagedVersion { value: value }
    }

    implements baml.ToString {
        function to_string(self) -> string {
            self.value
        }
    }
}

const SEMVER_COMPONENT_MAX = 18446744073709551615n;
const SEMVER_COMPONENT_MAX_DIGITS = 20;
const CHANNEL_NAMES: ChannelName[] = ["canary", "nightly"];

// The behavior lives on each selector, so adding a kind does not add branches
// to installation, update, and freshness dispatchers.
interface ToolchainSelector requires baml.ToString {
    function kind(self) -> ToolchainKind throws never;
    function ensure_installed(
        self,
        source: string,
        telemetry: WrapperTelemetry?,
    ) -> string throws WrapperFailure;
    function update(self, telemetry: WrapperTelemetry?) -> null throws WrapperFailure;
    function warn_if_outdated(self) -> bool throws never {
        false
    }
    function refresh_channel(self) -> null throws never {
        null
    }
}

class VersionSelector {
    value: string,

    function new(value: string) -> Self throws InvalidToolchainVersion {
        VersionSelector { value: ManagedVersion.new(value).value }
    }

    implements baml.ToString {
        function to_string(self) -> string {
            self.value
        }
    }

    implements ToolchainSelector {
        function kind(self) -> ToolchainKind {
            "version"
        }

        function ensure_installed(self, source: string, telemetry: WrapperTelemetry?) -> string {
            if (!installation_present(self.value)) {
                ReleaseStore.new().install(self.value, false, false, false, telemetry = telemetry);
            }
            verify_managed(self.value);
            `${toolchains()}/${self.value}/bin/${cli_name()}`
        }

        function update(self, telemetry: WrapperTelemetry?) -> null {
            baml.io.println(
                `active selector ${self.value} is an exact version and does not advance automatically`,
            );
            null
        }
    }
}

class ChannelSelector {
    value: ChannelName,

    implements baml.ToString {
        function to_string(self) -> string {
            self.value
        }
    }

    implements ToolchainSelector {
        function kind(self) -> ToolchainKind {
            "channel"
        }

        function ensure_installed(self, source: string, telemetry: WrapperTelemetry?) -> string {
            let concrete = ChannelState.new().recorded(self.value)
                ?? ReleaseStore.new().install(self.value, true, false, false, telemetry = telemetry);
            if (!installation_present(concrete)) {
                ChannelState.new().validate();
                ReleaseStore.new().install(concrete, false, false, false, telemetry = telemetry);
            }
            verify_managed(concrete);
            `${toolchains()}/${concrete}/bin/${cli_name()}`
        }

        function update(self, telemetry: WrapperTelemetry?) -> null {
            ReleaseStore.new().install(self.value, true, false, true, telemetry = telemetry);
            null
        }

        function warn_if_outdated(self) -> bool {
            self.warn_if_outdated_impl() catch (error) {
                WrapperFailure => false
            }
        }

        function refresh_channel(self) -> null {
            self.refresh_channel_impl() catch (error) {
                WrapperFailure => null
            }
        }
    }

    function warn_if_outdated_impl(self) -> bool {
        let current = ChannelState.new().recorded(self.value);
        let path = `${ReleaseStore.new().cache}/${self.value}.json`;
        if (current == null || !baml.fs.exists(path)) {
            return false;
        }
        let cached = baml.json.decode<ToolchainManifest>(baml.fs.read(path));
        version(cached.version);
        if (cached.channel == self.value && cached.version != current) {
            baml.io.eprintln(
                `warning: Your version of baml for toolchain: ${self.value} is outdated. Update it with baml toolchain update.`,
            );
            return true;
        }
        false
    }

    function refresh_channel_impl(self) -> null {
        let config = document(config_path());
        if let update: baml.toml.Table = table_at(config, "update", config_path()) {
            match (update.items.get("auto_check")) {
                null | true => {},
                _ => {
                    return null;
                },
            }
        }
        let cache = ReleaseStore.new().cache;
        baml.fs.mkdir(cache, baml.fs.MkdirOptions { recursive: true });
        let marker = `${cache}/${self.value}.last-check`;
        let lock = try_lock(`${marker}.lock`)
            ?? return null;
        defer { lock.close(); }
        let checked = if (baml.fs.exists(marker)) {
            bigint.parse(baml.fs.read(marker)) catch (error) {
                baml.errors.ParseError => null
            }
        } else {
            null
        };
        if let checked: bigint = checked {
            if (
                baml.time.Instant.from_timestamp_milliseconds(checked).elapsed().to_milliseconds()
                    < CHANNEL_REFRESH_INTERVAL.to_milliseconds()
            ) {
                return null;
            }
        }
        // Failed attempts count too, so an offline user does not pay a timeout on every launch.
        baml.fs.write(marker, baml.time.Instant.now().to_timestamp_milliseconds().to_string());
        ReleaseStore.new().manifest(
            self.value,
            true,
            timeout_seconds = PASSIVE_REFRESH_TIMEOUT_SECONDS,
        );

        null
    }
}

class PathSelector {
    value: string,

    implements baml.ToString {
        function to_string(self) -> string {
            self.value
        }
    }

    implements ToolchainSelector {
        function kind(self) -> ToolchainKind {
            "path"
        }

        function ensure_installed(self, source: string, telemetry: WrapperTelemetry?) -> string {
            verify_local(self.value, source);
            self.value
        }

        function update(self, telemetry: WrapperTelemetry?) -> null {
            baml.io.println(
                `active selector ${self.value} is a local path and does not advance automatically`,
            );
            null
        }
    }
}

const CHANNEL_REFRESH_INTERVAL = baml.time.Duration.from_hours(24);
const PASSIVE_REFRESH_TIMEOUT_SECONDS = 5;
