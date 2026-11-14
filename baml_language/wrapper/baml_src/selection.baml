class Selection {
    selector: ToolchainSelector,
    source: string,

    function new(value: string, source: string) -> Self {
        let selector: ToolchainSelector = match (value) {
            "canary" | "nightly" => ChannelSelector { value: value },
            _ => {
                if (is_path(value)) {
                    PathSelector { value: value }
                } else {
                    VersionSelector.new(value)
                }
            },
        };
        Selection { selector: selector, source: source }
    }

    function ensure_installed(self, telemetry: WrapperTelemetry? = null) -> string {
        self.selector.ensure_installed(self.source, telemetry)
    }

    function warn_if_outdated(self) -> bool {
        self.selector.warn_if_outdated()
    }
    function refresh_channel(self) -> null {
        self.selector.refresh_channel()
    }
}
