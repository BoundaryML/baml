// Passive metadata checks must not turn an installed toolchain into a network requirement.
function warn_if_outdated(active: Selection) -> bool {
    warn_if_outdated_impl(active) catch (_) {
        _ => false
    }
}

function warn_if_outdated_impl(active: Selection) -> bool {
    if (!channel(active.value)) {
        return false;
    }
    let current = recorded(active.value);
    let path = `${cache_dir()}/${active.value}.json`;
    if (current == null || !baml.fs.exists(path)) {
        return false;
    }
    let cached = baml.json.decode<ToolchainManifest>(baml.fs.read(path));
    version(cached.version);
    if (cached.channel == active.value && cached.version != current) {
        baml.io.eprintln(
            `warning: Your version of baml for toolchain: ${active.value} is outdated. Update it with baml toolchain update.`,
        );
        return true;
    }
    false
}

function refresh_channel(active: Selection) -> null {
    refresh_channel_impl(active) catch (_) {
        _ => null
    }
}

function refresh_channel_impl(active: Selection) -> null {
    if (!channel(active.value)) {
        return null;
    }
    let config = document(config_path());
    if let update: baml.toml.Table = table_at(config, "update", config_path()) {
        match (update.items.get("auto_check")) {
            null | true => {},
            _ => {
                return null;
            },
        }
    }
    let cache = cache_dir();
    baml.fs.mkdir(cache, baml.fs.MkdirOptions { recursive: true });
    let marker = `${cache}/${active.value}.last-check`;
    let lock = baml.fs.try_lock(`${marker}.lock`);
    if let lock: baml.fs.File = lock {
        defer { lock.close(); }
        let now = baml.time.Instant.now().to_timestamp_milliseconds();
        let checked = bigint.parse(read_text(marker)) catch (_) {
            _ => 0n
        };
        if (now - checked < 86400000n) {
            return null;
        }
        // Throttle failed attempts too; an offline user should not pay a timeout on every command.
        baml.fs.write(marker, now.to_string());
        manifest(active.value, true, timeout_seconds = 5);
    }
    null
}
