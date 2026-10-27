function self_update() -> null {
    if (!self_update_enabled()) {
        fail(
            "self-update is disabled in this build.\nUpdate BAML with your package manager.\nHomebrew: brew upgrade baml\nWinGet: winget upgrade --id BoundaryML.BAML\nArch Linux: sudo pacman -Syu baml\nTo check language toolchain updates, run: baml toolchain status",
        );
    }
    let release = baml.json.decode<WrapperManifest>(fetch_text(`${manifest_base()}/wrapper.json`));
    validate_artifacts(release.schema, release.artifacts);
    version(release.version);
    let artifact = release.artifacts[baml.sys.host_target()];
    let data = download(artifact);
    let format = if (artifact.url.ends_with(".zip")) {
        "zip"
    } else {
        "tar.gz"
    };
    let entries = baml.archive.entries(data, format);
    let name = if (windows()) {
        "bin/baml.exe"
    } else {
        "bin/baml"
    };
    ensure(entries.includes(name), "wrapper archive missing bin/baml");
    let current = baml.sys.executable();
    let staging = `${current}.unpacked`;
    baml.fs.mkdir(staging, baml.fs.MkdirOptions { recursive: false });
    defer { baml.fs.remove_dir_all(staging); }
    baml.archive.extract(data, format, staging);
    let temp = if (windows() && current.ends_with(".exe")) {
        current.slice(0, current.length() - 4) + ".tmp"
    } else {
        `${current}.tmp`
    };
    baml.fs.rename(`${staging}/${name}`, temp);
    if (windows()) {
        let helper = `${current}.updater.exe`;
        let file = baml.fs.open(current, "r");
        defer { file.close(); }
        baml.fs.write_bytes(helper, file.bytes());
        baml.sys.start_detached(helper, ["--replace", temp, current]);
    } else {
        baml.fs.rename(temp, current);
    }
    baml.io.println(`updated BAML wrapper to ${release.version}`);
    baml.io.println("To check language toolchain updates, run: baml toolchain status");
    null
}

function replace_wrapper(args: string[]) -> null {
    ensure(args.length() == 2, "--replace requires source and destination");
    let attempts = 0;
    while (attempts < 100) {
        baml.fs.rename(args[0], args[1]) catch (error) {
            _ => {
                attempts += 1;
                baml.sys.sleep(baml.time.Duration.from_milliseconds(50));
                continue;
            },
        };
        return null;
    }
    fail(`failed to replace wrapper ${args[1]} after bounded wait`)
}
