// A release source is validated once; manifests, cache paths, and channel
// mappings all use that same source identity.
const DEFAULT_MANIFEST_BASE = "https://pkg.boundaryml.com/manifest/v1";
const DOWNLOAD_TIMEOUT_SECONDS = 30;
const MANIFEST_SCHEMA_VERSION = 1;

class ReleaseStore {
    base: string,
    cache: string,

    function new() -> Self {
        let base = (baml.env.get("BAML_MANIFEST_BASE_URL") ?? DEFAULT_MANIFEST_BASE).trim();
        while (base.ends_with("/")) {
            base = base.slice(0, base.length() - 1);
        }
        ensure(secure_url(base), "manifest base URL must use HTTPS (HTTP is allowed only on loopback)");
        ReleaseStore {
            base: base,
            cache: `${baml_home()}/manifest-cache/baml/${hash(base.to_utf8())}`,
        }
    }

    function manifest(
        self,
        selector: string,
        force_remote: bool,
        timeout_seconds: int = DOWNLOAD_TIMEOUT_SECONDS,
    ) -> ToolchainManifest {
        validate_selector(selector);
        ensure(!is_path(selector), "local paths have no release manifest");
        baml.sys.host_target();
        let relative = if (channel(selector)) {
            `${selector}.json`
        } else {
            `version/${selector}.json`
        };
        let cached = `${self.cache}/${relative}`;
        let text = if (!force_remote && !channel(selector) && baml.fs.exists(cached)) {
            baml.fs.read(cached)
        } else {
            fetch_text(`${self.base}/${relative}`, timeout_seconds = timeout_seconds)
        };
        let result = parse_release<ToolchainManifest>(text);
        ensure(
            if (channel(selector)) {
                result.channel == selector
            } else {
                result.version == selector
            },
            `manifest does not match requested ${selector}: ${result.channel}/${result.version}`,
        );
        // Cache only validated metadata, separated by release source.
        baml.fs.mkdir(`${self.cache}/version`, baml.fs.MkdirOptions { recursive: true });
        write_atomic(cached, text);
        write_atomic(`${self.cache}/version/${result.version}.json`, text);
        if (channel(selector)) {
            write_atomic(
                `${self.cache}/${selector}.last-check`,
                baml.time.Instant.now().to_timestamp_milliseconds().to_string(),
            );
        }
        result
    }

    function install(
        self,
        selector: string,
        activate: bool,
        force: bool,
        force_remote: bool,
        telemetry: WrapperTelemetry? = null,
    ) -> string {
        let result = self.install_impl(selector, activate, force, force_remote) catch (error) {
            _ => {
                telemetry_event(
                    telemetry,
                    "wrapper_install",
                    fields = {
                        "outcome": "failure",
                        "forced": force,
                        "toolchain_kind": telemetry_kind(selector),
                    },
                );
                throw error;
            }
        };
        telemetry_event(
            telemetry,
            "wrapper_install",
            fields = { "outcome": "success", "forced": force, "toolchain_kind": telemetry_kind(selector) },
        );
        result
    }

    function install_impl(
        self,
        selector: string,
        activate: bool,
        force: bool,
        force_remote: bool,
    ) -> string {
        validate_selector(selector);
        ensure(!is_path(selector), `${selector} is a local path; there is nothing to install`);
        if (activate && channel(selector)) {
            ChannelState.new().validate();
        }
        let release = self.manifest(selector, force_remote);
        release.artifacts[baml.sys.host_target()].install(release.version, force);
        if (activate && channel(selector)) {
            self.record_channel(selector, release);
        }
        baml.io.eprintln(`installed BAML toolchain ${release.version}`);
        release.version
    }

    function record_channel(self, name: string, release: ToolchainManifest) -> null {
        ChannelState.new().validate();
        let fields: map<string, string> = {
            "active_version": release.version,
            "resolved_at": release.released_at,
            "manifest_path": `${self.cache}/version/${release.version}.json`,
            "manifest_base_url": self.base,
        };
        let text = fields.keys().reduce(
            (text: string, key: string) -> string {
                edit_config(text, ["channels", name, key], fields[key])
            },
            read_text(state_path()),
        );
        ChannelState.new().validate_text(text);
        write_atomic(state_path(), text);
        null
    }
}
