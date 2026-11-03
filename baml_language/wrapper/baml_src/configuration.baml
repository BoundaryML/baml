class ProjectConfiguration {
    path: string,
    text: string,

    function new(path: string, text: string) -> Self {
        let config = ProjectConfiguration { path: path, text: text };
        config.validate();
        config
    }

    function validate(self) -> null {
        let doc = baml.toml.Table.parse(self.text);
        baml.json.to<ProjectShape>(doc.to_json(), naming = baml.json.Naming.Aliased);
        // JSON renders TOML datetimes as strings. Check string-valued fields in
        // the original tree too, so a date cannot masquerade as a valid setting.
        for (let section in ["pack", "bridge", "boundary"]) {
            if let table: baml.toml.Table = table_at(doc, section, self.path) {
                validate_strings(
                    table,
                    if (section == "boundary") {
                        ["project", "api_url"]
                    } else {
                        ["telemetry_environment"]
                    },
                    self.path,
                );
            }
        }
        if let dependencies: baml.toml.Table = table_at(doc, "dependencies", self.path) {
            for (let key in dependencies.items.keys()) {
                if let dependency: baml.toml.Table = table_at(dependencies, key, self.path) {
                    validate_strings(dependency, ["path"], self.path);
                }
            }
        }
        if let generators: baml.toml.Table = table_at(doc, "generator", self.path) {
            for (let key in generators.items.keys()) {
                if let generator_options: baml.toml.Table = table_at(generators, key, self.path) {
                    validate_strings(
                        generator_options,
                        ["output_type", "naming_convention", "output_dir", "sdk_import_path"],
                        self.path,
                    );
                }
            }
        }
        if let scripts: baml.toml.Table = table_at(doc, "scripts", self.path) {
            for (let key in scripts.items.keys()) {
                match (scripts.items[key]) {
                    string => {},
                    let tokens: baml.toml.Item[] => {
                        validate_string_items(tokens, self.path);
                    },
                    _ => {
                        ensure(false, `${self.path}: scripts.${key} must be a string or string array`);
                    },
                }
            }
        }
        if let test_options: baml.toml.Table = table_at(doc, "test", self.path) {
            validate_strings(test_options, ["default"], self.path);
            if let profiles: baml.toml.Table = table_at(test_options, "profiles", self.path) {
                for (let key in profiles.items.keys()) {
                    if let profile: baml.toml.Table = table_at(profiles, key, self.path) {
                        match (profile.items.get("args")) {
                            null => {},
                            let tokens: baml.toml.Item[] => {
                                validate_string_items(tokens, self.path);
                            },
                            _ => {
                                ensure(false, `${self.path}: test profile args must be a string array`);
                            },
                        }
                    }
                }
            }
        }
        if let package: baml.toml.Table = table_at(doc, "package", self.path) {
            string_at(package, "name", self.path);
            match (package.items.get("prelude")) {
                null | bool => {},
                _ => {
                    ensure(false, `${self.path}: package.prelude must be a boolean`);
                },
            }
        }
        if let toolchain: baml.toml.Table = table_at(doc, "toolchain", self.path) {
            for (let key in ["version", "channel", "path"]) {
                if let value: string = string_at(toolchain, key, self.path) {
                    if (key == "version") {
                        version(value);
                    }
                    if (key == "channel") {
                        ensure(channel(value), `${self.path}: invalid toolchain.channel`);
                    }
                    if (key == "path") {
                        ensure(value.trim() != "", `${self.path}: empty toolchain.path`);
                    }
                }
            }
        }
        null
    }
}

class UserConfiguration {
    text: string,

    function new(text: string) -> Self {
        let config = UserConfiguration { text: text };
        config.validate();
        config
    }

    function validate(self) -> null {
        let doc = baml.toml.Table.parse(self.text);
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
}

class ChannelState {
    path: string,

    function new() -> Self {
        ChannelState { path: state_path() }
    }

    function validate(self) -> null {
        self.validate_text(read_text(self.path))
    }

    function validate_text(self, text: string) -> null {
        let doc = baml.toml.Table.parse(text);
        if let channels: baml.toml.Table = table_at(doc, "channels", self.path) {
            for (let key in channels.items.keys()) {
                let value = table_at(channels, key, self.path);
                if let value: baml.toml.Table = value {
                    validate_channel(value, `${self.path}: channels.${key}`);
                }
            }
        }
        null
    }

    function recorded(self, name: string) -> string? {
        let doc = document(self.path);
        if let channels: baml.toml.Table = table_at(doc, "channels", self.path) {
            if let entry: baml.toml.Table = table_at(channels, name, self.path) {
                let base = string_at(entry, "manifest_base_url", self.path);
                ensure(
                    base == null || base == ReleaseStore.new().base,
                    `${self.path}: channel ${name} was selected from a different manifest source`,
                );
                let active = string_at(entry, "active_version", self.path)
                    ?? throw MissingConfigurationField { path: self.path, fields: ["channels", name, "active_version"] };
                version(active);
                return active;
            }
        }
        null
    }

    function source_matches(self, name: string) -> bool {
        let doc = document(self.path);
        if let channels: baml.toml.Table = table_at(doc, "channels", self.path) {
            if let entry: baml.toml.Table = table_at(channels, name, self.path) {
                let base = string_at(entry, "manifest_base_url", self.path);
                return base == null || base == ReleaseStore.new().base;
            }
        }
        false
    }
}
