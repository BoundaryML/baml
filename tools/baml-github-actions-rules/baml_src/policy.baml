function action_base(reference: string) -> string {
    reference.split("@")[0].to_lower_case().trim_end().replace_all("\\", "/")
}

function lint_action(
    path: string,
    location: string,
    reference: string,
    step: Json,
    ds: Diagnostic[],
) -> void {
    let base = action_base(reference);
    if (base == "" || reference.includes("${{")) {
        emit(
            ds,
            path,
            location,
            "ACTION",
            "Action references must be literal and nonempty so their setup behavior can be audited.",
        );
        return;
    }
    if (base.starts_with("./")) {
        let canonical = base.slice(2, base.length())
            .split("/")
            .filter((s) -> {
                s != "." && s != ""
            })
            .join("/");
        if (canonical.split("/").includes("..")) {
            emit(ds, path, location, "ACTION", "Local action paths must not contain parent traversal.");
        }
        if (
            v0_exclusions().get(`${canonical}/action.yml`) != null
                || v0_exclusions().get(`${canonical}/action.yaml`) != null
        ) {
            emit(
                ds,
                path,
                location,
                "V0-ACTION",
                "An in-scope file must not use an excluded v0 action; use setup-mise and the R2 sccache convention.",
            );
        }
        return;
    }
    // Only the actual two bootstrap actions inside setup-mise are exempt.
    if (
        path == ".github/actions/setup-mise/action.yml"
            && ["jdx/mise-action", "cargo-bins/cargo-binstall"].includes(base)
    ) {
        return;
    }
    // rustup is intentionally separate from mise in ci.yaml. This action is the
    // canonical cross-platform rustup bootstrap, not an alternative tool manager.
    if (path == ".github/actions/setup-rust/action.yml" && base == "dtolnay/rust-toolchain") {
        return;
    }
    if (base == "boundaryml/setup-baml") {
        return;
    }
    if (
        base == "swatinem/rust-cache"
            || base == "mozilla-actions/sccache-action"
            || base.includes("rust-cache")
            || base.includes("sccache")
    ) {
        emit(
            ds,
            path,
            location,
            "RUST-CACHE",
            "Use setup-mise for sccache/direnv and load the repository .envrc R2 configuration (or setup-sccache); alternative Rust caches are forbidden.",
        );
        return;
    }
    if (base == "actions/cache" || base.starts_with("actions/cache/")) {
        let paths = string_value(field(field(step, "with"), "path")).to_lower_case().replace_all("\\", "/");
        if (
            paths == ""
                || paths.includes("${{")
                || paths.includes(".cargo")
                || paths.includes("sccache")
                || paths.lines().some((line) -> {
                    line.trim().split("/").includes("target")
                })
                || paths.includes("/target")
        ) {
            emit(
                ds,
                path,
                location,
                "RUST-CACHE",
                "Generic caches must have literal non-Rust paths. Cargo state and compilation caches must use the repository R2 sccache setup.",
            );
        }
        return;
    }
    if (base == "pyo3/maturin-action") {
        let config = field(step, "with");
        if (
            field(config, "command") != "build"
                || (
                    field(config, "sccache") != null
                        && field(config, "sccache") != false
                        && field(config, "sccache") != "false"
                )
        ) {
            emit(
                ds,
                path,
                location,
                "TOOL-ACTION",
                "maturin-action is approved only as a wheel build operation, with its alternative cache disabled; install standalone maturin through mise.",
            );
        }
        return;
    }
    // Known non-setup operations. New external actions require semantic review;
    // new workflow/action FILES are always discovered without an opt-in list.
    let operations = [
        "actions/checkout",
        "useblacksmith/checkout",
        "actions/upload-artifact",
        "actions/upload-artifact/merge",
        "actions/download-artifact",
        "actions/github-script",
        "actions/configure-pages",
        "actions/upload-pages-artifact",
        "actions/deploy-pages",
        "peter-evans/create-pull-request",
        "marocchino/sticky-pull-request-comment",
        "bobheadxi/deployments",
        "sebrollen/toml-action",
        "codspeedhq/action",
        "pypa/gh-action-pypi-publish",
        "nuget/login",
        "rust-lang/crates-io-auth-action",
        "aws-actions/configure-aws-credentials",
        "softprops/action-gh-release",
        "docker/build-push-action",
        "pyo3/maturin-action",
    ];
    if (!operations.includes(base)) {
        emit(
            ds,
            path,
            location,
            "TOOL-ACTION",
            `Unapproved action ${reference}. Install tools through ./.github/actions/setup-mise; BoundaryML/setup-baml is approved for BAML. Review genuinely non-setup actions explicitly.`,
        );
    }
}

function lint_steps(path: string, location: string, steps: Json, ds: Diagnostic[]) -> void {
    if (!(steps is Json[])) {
        throw baml.errors.InvalidArgument { message: `${location} must be a sequence` };
    }
    let cache_guards: string[] = [];
    let cache_tools = false;
    let index = 0;
    for (let step in array(steps)) {
        let where = `${location}[${index}] (${string_value(field(step, "name"))})`;
        index += 1;
        if (!(step is map<string, Json>)) {
            emit(ds, path, where, "STRUCTURE", "Each step must be a mapping.");
            continue;
        }
        if ((field(step, "uses") == null) == (field(step, "run") == null)) {
            emit(ds, path, where, "STRUCTURE", "A step must declare exactly one of uses or run.");
        }
        validate_cache_env(path, `${where}.env`, field(step, "env"), ds);
        let guard = condition(step);
        let cache_ready = cache_guards.includes("") || (guard != "false" && cache_guards.includes(guard));
        if (field(step, "uses") != null) {
            let reference = string_value(field(step, "uses"));
            lint_action(path, `${where}.uses`, reference, step, ds);
            let base = action_base(reference);
            if (base == "./.github/actions/setup-mise") {
                let args = string_value(field(field(step, "with"), "install_args"));
                if (
                    guard == ""
                        && (
                            args == ""
                                || (
                                    args.split(" ").includes("sccache")
                                        && args.split(" ").includes("direnv")
                                )
                        )
                ) {
                    cache_tools = true;
                }
            }
            if (
                (
                    base == "./.github/actions/setup-sccache"
                        || base == "./.github/actions/setup-rust"
                )
                    && (
                        field(step, "continue-on-error") == null
                            || field(step, "continue-on-error") == false
                    )
            ) {
                cache_guards.push(guard);
            }
        }
        if (field(step, "run") != null) {
            if (!(field(step, "run") is string)) {
                emit(ds, path, where, "STRUCTURE", "run must be a string.");
                continue;
            }
            let run = string_value(field(step, "run"));
            let commands = shell_commands(run);
            for (let command in commands) {
                if (installs_tool(command)) {
                    emit(
                        ds,
                        path,
                        `${where}.run`,
                        "TOOL-INSTALL",
                        `Install tools via setup-mise, not '${executable_words(command).join(" ")}'. Project dependency installs and tool usage are allowed.`,
                    );
                }
                if (rust_build(command) && !cache_ready && !bootstrap_compile(path, step, command)) {
                    emit(
                        ds,
                        path,
                        `${where}.run`,
                        "RUST-SETUP",
                        "Before compiling Rust, load the repository R2 sccache setup (.github/actions/setup-sccache or mise sccache/direnv followed by direnv allow .envrc and direnv export gha).",
                    );
                    break;
                }
            }
            if (
                cache_tools
                    && commands.some((c) -> {
                        executable_words(c) == ["direnv", "allow", ".envrc"]
                    })
                    && commands.some((c) -> {
                        executable_words(c) == ["direnv", "export", "gha", ">>", "$GITHUB_ENV"]
                    })
            ) {
                cache_guards.push(guard);
            }
        }
    }
}

function condition(step: Json) -> string {
    let value = field(step, "if");
    if (value == null || value == true) {
        ""
    } else {
        baml.json.stringify(value)
    }
}

function bootstrap_compile(path: string, step: Json, words: string[]) -> bool {
    path == ".github/actions/setup-sccache/action.yml"
        && string_value(field(step, "name")) == "Build native Windows cache wrapper"
        && string_value(field(step, "if")) == "runner.os == 'Windows'"
        && string_value(field(step, "shell")) == "bash"
        && words == ["RUSTC_WRAPPER=", "cargo", "build", "-p", "tools_sccache"]
}

function validate_cache_env(path: string, where: string, value: Json, ds: Diagnostic[]) -> void {
    for (let key in object(value).keys()) {
        if (
            [
                "SCCACHE_GHA_ENABLED",
                "SCCACHE_BUCKET",
                "SCCACHE_ENDPOINT",
                "SCCACHE_REGION",
                "SCCACHE_S3_KEY_PREFIX",
                "RUSTC_WRAPPER",
            ]
                .includes(key)
        ) {
            emit(
                ds,
                path,
                `${where}.${key}`,
                "CACHE-CONFIG",
                "Keep Rust wrapper/backend configuration in .envrc; workflows must not override it.",
            );
        }
    }
}

function validate_canonical(path: string, doc: Json, ds: Diagnostic[]) -> void {
    if (path == ".github/actions/setup-mise/action.yml") {
        let steps = array(field(field(doc, "runs"), "steps"));
        let mise = steps.find((s) -> {
            action_base(string_value(field(s, "uses"))) == "jdx/mise-action"
        });
        if (mise == null) {
            emit(ds, path, "runs.steps", "BOOTSTRAP", "setup-mise must bootstrap jdx/mise-action.");
        } else {
            let config = field(mise, "with");
            if (
                condition(mise) != ""
                    || field(config, "add_shims_to_path") != false
                    || field(config, "cache_save") != "${{ github.ref == 'refs/heads/canary' }}"
                    || field(config, "install_args") != "${{ inputs.install_args }}"
                    || field(config, "cache_key_prefix") != "mise-baml3"
                    || field(config, "fetch_from_github") != false
            ) {
                emit(
                    ds,
                    path,
                    "runs.steps.with",
                    "BOOTSTRAP",
                    "Preserve canonical mise inputs, mise-baml3 cache prefix, canary-only saves, direct download, and disabled shims.",
                );
            }
        }
        if (
            !steps.some((s) -> {
                string_value(field(s, "run")).includes(
                    "echo \"MISE_TASK_RUN_AUTO_INSTALL=false\" >> \"$GITHUB_ENV\"",
                )
            })
        ) {
            emit(
                ds,
                path,
                "runs.steps",
                "BOOTSTRAP",
                "setup-mise must disable automatic installation by mise tasks.",
            );
        }
    }
    if (path == ".github/actions/setup-rust/action.yml") {
        if (
            !array(field(field(doc, "runs"), "steps")).some((s) -> {
                field(s, "uses") == "./.github/actions/setup-sccache" && condition(s) == ""
            })
        ) {
            emit(
                ds,
                path,
                "runs.steps",
                "BOOTSTRAP",
                "setup-rust must unconditionally configure repository sccache.",
            );
        }
    }
    if (path == ".github/actions/setup-sccache/action.yml") {
        let steps = array(field(field(doc, "runs"), "steps"));
        if (
            !steps.some((s) -> {
                field(s, "uses") == "./.github/actions/setup-mise"
                    && field(field(s, "with"), "install_args") == "sccache direnv"
                    && condition(s) == ""
            })
                || !steps.some((s) -> {
                    field(s, "run")
                        == "set -euo pipefail\ndirenv allow .envrc\ndirenv export gha >> \"$GITHUB_ENV\"\n"
                        && condition(s) == ""
                })
        ) {
            emit(
                ds,
                path,
                "runs.steps",
                "BOOTSTRAP",
                "setup-sccache must install sccache/direnv through mise and export the existing .envrc configuration unconditionally.",
            );
        }
    }
}

function local_references(doc: Json) -> string[] {
    let refs: string[] = [];
    if let m: map<string, Json> = doc {
        for (let key in m.keys()) {
            let value = field(doc, key);
            if (key == "uses" && string_value(value).starts_with("./")) {
                refs.push(string_value(value));
            }
            refs = refs.concat(local_references(value));
        }
    } else if let items: Json[] = doc {
        for (let item in items) {
            refs = refs.concat(local_references(item));
        }
    }
    refs
}

function audit_repository(sources: Source[], ds: Diagnostic[]) -> void {
    for (let source in sources) {
        if (v0_exclusions().get(source.path) != null) {
            continue;
        }
        let doc = baml.yaml.parse(source.text) catch (e) {
            baml.yaml.ParseError => null
        };
        if (doc == null) {
            continue;
        }
        for (let reference in local_references(doc)) {
            let path = reference.slice(2, reference.length());
            let target = sources.find((s) -> {
                s.path == path || s.path == `${path}/action.yml` || s.path == `${path}/action.yaml`
            });
            if (target == null) {
                emit(
                    ds,
                    source.path,
                    "uses",
                    "LOCAL-ACTION",
                    `Cannot inspect ${reference}: the workflow/action manifest must exist in the discovered repository files.`,
                );
            }
        }
        for (let name in object(field(doc, "jobs")).keys()) {
            let job = field(field(doc, "jobs"), name);
            let steps = array(field(job, "steps"));
            let cached = steps.some((s) -> {
                ["./.github/actions/setup-sccache", "./.github/actions/setup-rust"].includes(
                    string_value(field(s, "uses")),
                )
                    || string_value(field(s, "run")).includes("direnv export gha")
            });
            if (cached) {
                for (let key in ["BAML_SCCACHE_R2_ACCESS_KEY_ID", "BAML_SCCACHE_R2_SECRET_ACCESS_KEY"]) {
                    let value = field(field(job, "env"), key) ?? field(field(doc, "env"), key);
                    if (string_value(value) != "${{ secrets." + key + " }}") {
                        emit(
                            ds,
                            source.path,
                            `jobs.${name}.env.${key}`,
                            "R2-CREDENTIALS",
                            "Bind the optional repository secret of the same name so .envrc uses R2 when credentials exist and the local fallback on forks.",
                        );
                    }
                }
            }
            let ref = string_value(field(job, "uses"));
            if (ref.starts_with("./")) {
                let target = sources.find((s) -> {
                    s.path == ref.slice(2, ref.length())
                });
                if let target: Source = target {
                    if (v0_exclusions().get(target.path) != null) {
                        continue;
                    }
                    let called = baml.yaml.parse(target.text) catch (e) {
                        baml.yaml.ParseError => null
                    };
                    if (
                        field(
                            field(field(field(called, "on"), "workflow_call"), "secrets"),
                            "BAML_SCCACHE_R2_ACCESS_KEY_ID",
                        )
                            != null
                    ) {
                        let secrets = field(job, "secrets");
                        if (
                            secrets != "inherit"
                                && (
                                    field(secrets, "BAML_SCCACHE_R2_ACCESS_KEY_ID") == null
                                        || field(secrets, "BAML_SCCACHE_R2_SECRET_ACCESS_KEY")
                                            == null
                                )
                        ) {
                            emit(
                                ds,
                                source.path,
                                `jobs.${name}.secrets`,
                                "R2-CREDENTIALS",
                                "Forward both R2 secrets to this reusable workflow (or inherit secrets).",
                            );
                        }
                    }
                }
            }
        }
    }
}
