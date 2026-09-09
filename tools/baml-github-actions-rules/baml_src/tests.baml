function workflow(steps: string) -> string {
    "name: fixture\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n" + steps
}

function diagnostics(text: string, path: string = ".github/workflows/future.yml") -> Diagnostic[] {
    lint_sources([Source { path, text }]).diagnostics
}

function rules(text: string, path: string = ".github/workflows/future.yml") -> string[] {
    diagnostics(text, path = path).map((d) -> {
        d.rule
    })
}

function command_violation(script: string) -> bool {
    shell_commands(script).some((w) -> {
        installs_tool(w)
    })
}

test "reject alternative setup actions including unknown vendors and SHA pins" {
    for (
        let action in [
            "Swatinem/rust-cache@v2",
            "Swatinem/rust-cache@abcdef123",
            "actions/setup-node@v7",
            "pnpm/action-setup@v6",
            "astral-sh/setup-uv@v8",
            "jdx/mise-action@v4",
            "cargo-bins/cargo-binstall@main",
            "ruby/setup-ruby@v1",
            "dtolnay/rust-toolchain@stable",
            "taiki-e/install-action@v2",
            "unknown/innocent-name@v1",
            "mozilla-actions/sccache-action@v0.0.9",
        ]
    ) {
        assert.equal(diagnostics(workflow(`      - uses: ${action}\n`)).length(), 1);
    }
}

test "approved BAML setup and ordinary operations do not require mise" {
    assert.equal(
        rules(
            workflow(
                "      - uses: BoundaryML/setup-baml@v1\n        with: {toolchain: nightly}\n      - run: baml check\n      - uses: actions/upload-artifact@v7\n",
            ),
        ),
        [],
    );
    assert.equal(rules(workflow("      - uses: boundaryml/setup-baml@deadbeef\n")), [])
}

test "installation commands are recognized through whitespace quotes and wrappers" {
    for (
        let script in [
            "curl -fsSL https://astral.sh/uv/install.sh | sh",
            "wget https://mise.run -O- | sh",
            "irm https://astral.sh/uv/install.ps1 | iex",
            "sudo apt-get install -y nodejs",
            "brew install uv",
            "baml toolchain use nightly",
            "npm i -fg pnpm",
            "cargo install foo",
            "cargo +stable install --locked foo",
            "sudo env CI=true /usr/bin/cargo install foo",
            "cargo\tinstall foo",
            "cargo \\\n install foo",
            "uv tool install ruff",
            "uv python install 3.13",
            "npm i --global pnpm",
            "npm --global install uv",
            "pnpm add -g foo",
            "yarn global add foo",
            "python3 -m pip install uv",
            "pip install ruff==1",
            "corepack enable",
            "mise install uv",
            "mise use node@24",
            "go install example.org/tool@latest",
            "dotnet tool install --global x",
            "echo hi; cargo install foo",
            "echo hi # comment\ncargo install foo",
            "OUT=$(cargo install foo)",
            "bash -c 'uv tool install ruff'",
            "'cargo' 'install' foo",
        ]
    ) {
        assert.is_true(command_violation(script));
    }
}

test "using tools and installing project dependencies is permitted" {
    for (
        let script in [
            "uv run lint.py",
            "uv sync --frozen",
            "uv pip install -r requirements.txt",
            "pnpm install --frozen-lockfile",
            "npm ci",
            "npm install --save-dev vitest",
            "npm exec --package=tool -- tool check",
            "pnpm exec playwright install --with-deps chromium",
            "cargo test",
            "cargo fetch",
            "cargo run --bin baml-cli",
            "mise exec uv -- uv run check.py",
            "rustup toolchain install nightly",
            "rustup target add wasm32-unknown-unknown",
            "echo 'cargo install foo'",
            "printf '%s' 'uv tool install ruff'",
            "# cargo install foo\nuv --version",
            "command -v uv",
            "git diff -- '.github/actions/setup-node/**'",
        ]
    ) {
        assert.equal(command_violation(script), false);
    }
}

test "new YAML and YML workflows and action files are included by default" {
    for (
        let path in [
            ".github/workflows/new.yml",
            ".github/workflows/new.yaml",
            ".github/workflows/build-cli-release-new.reusable.yaml",
            ".github/actions/new/nested/action.yml",
            ".github/actions/new/action.yaml",
            "tools/other/action.yml",
        ]
    ) {
        let text = if (path.includes("/action.")) {
            "name: future\nruns:\n  using: composite\n  steps:\n    - uses: actions/setup-node@v7\n"
        } else {
            workflow("      - uses: actions/setup-node@v7\n")
        };
        assert.equal(rules(text, path = path), ["TOOL-ACTION"]);
    }
}

test "every exact legacy exclusion is honored and sibling files are not excluded" {
    for (let path in v0_exclusions().keys()) {
        assert.is_true((v0_exclusions().get(path) ?? "").length() > 30);
        let report = lint_sources([Source { path, text: "invalid: [" }]);
        assert.equal(report.excluded, [path]);
        assert.equal(report.diagnostics.length(), 0);
        assert.equal(
            lint_sources([Source { path: path + ".renamed.yaml", text: "invalid: [" }])
                .diagnostics[0]
                .rule,
            "YAML",
        );
    }
}

test "stale exclusion entries are errors rather than silent coverage changes" {
    let result = lint_sources([], audit_exclusions = true);
    assert.equal(result.diagnostics.length(), v0_exclusions().length());
    assert.is_true(
        result.diagnostics.every((d) -> {
            d.rule == "EXCLUSION"
        }),
    )
}

test "in-scope workflows cannot delegate setup to excluded v0 actions" {
    assert.equal(rules(workflow("      - uses: ./.github/actions/setup-node\n")), ["V0-ACTION"]);
    assert.equal(rules(workflow("      - uses: ./.github/actions/./setup-node/\n")), ["V0-ACTION"]);
    assert.equal(
        rules(workflow("      - uses: ./.github/actions/../actions/setup-mise\n")),
        ["ACTION"],
    )
}

test "YAML comments quoted keys flow maps and aliases are semantic" {
    assert.equal(
        rules(
            workflow(
                "      # uses: actions/setup-node@v7\n      - run: |\n          echo 'uses: actions/setup-node@v7'\n",
            ),
        ),
        [],
    );
    assert.equal(rules(workflow("      - {'uses': 'actions/setup-node@v7'}\n")), ["TOOL-ACTION"]);
    assert.equal(
        rules(workflow("      - &bad {uses: actions/setup-node@v7}\n      - *bad\n")),
        ["TOOL-ACTION", "TOOL-ACTION"],
    );
    assert.equal(
        rules(workflow("      - run: >-\n          uv tool\n          install ruff\n")),
        ["TOOL-INSTALL"],
    )
}

test "malformed YAML duplicate keys documents and invalid shapes fail closed" {
    for (
        let text in [
            "jobs: [",
            "jobs: {}\njobs: {}",
            "jobs: {}\n---\njobs: {}",
            "[]",
            "jobs: {}",
            "jobs: {build: {steps: wrong}}",
            workflow("      - 123\n"),
            workflow("      - run: 12\n"),
        ]
    ) {
        assert.is_true(diagnostics(text).length() > 0);
    }
}

test "diagnostics identify file job step field rule and remedy" {
    let d = diagnostics(workflow("      - name: Bad bootstrap\n        uses: actions/setup-node@v7\n"))[0];
    assert.equal(d.file, ".github/workflows/future.yml");
    assert.contains(d.location, "jobs.build.steps[0] (Bad bootstrap).uses");
    assert.equal(d.rule, "TOOL-ACTION");
    assert.contains(d.message, "setup-mise")
}

test "Rust builds require cache setup earlier in the same job" {
    assert.equal(rules(workflow("      - run: cargo test\n")), ["RUST-SETUP"]);
    assert.equal(
        rules(workflow("      - run: cargo build\n      - uses: ./.github/actions/setup-sccache\n")),
        ["RUST-SETUP"],
    );
    assert.equal(
        rules(workflow("      - uses: ./.github/actions/setup-sccache\n      - run: cargo test\n")),
        [],
    );
    assert.equal(
        rules(workflow("      - uses: ./.github/actions/setup-rust\n      - run: cargo +stable build\n")),
        [],
    );
    assert.equal(
        rules(
            workflow("      - uses: ./.github/actions/setup-sccache\n")
                + "  another:\n    steps:\n      - run: cargo build\n",
        ),
        ["RUST-SETUP"],
    )
}

test "Miri interpretation and Cargo metadata do not require compilation caching" {
    assert.equal(
        rules(
            workflow(
                "      - run: cargo +nightly miri test\n      - run: cargo fetch\n      - run: cargo metadata --format-version 1\n",
            ),
        ),
        [],
    )
}

test "conditional setup cannot satisfy an unconditional or differently guarded build" {
    assert.equal(
        rules(
            workflow(
                "      - uses: ./.github/actions/setup-sccache\n        if: false\n      - run: cargo build\n",
            ),
        ),
        ["RUST-SETUP"],
    );
    assert.equal(
        rules(
            workflow(
                "      - uses: ./.github/actions/setup-sccache\n        if: runner.os == 'Linux'\n      - run: cargo build\n        if: runner.os == 'Windows'\n",
            ),
        ),
        ["RUST-SETUP"],
    );
    assert.equal(
        rules(
            workflow(
                "      - uses: ./.github/actions/setup-sccache\n        if: runner.os == 'Linux'\n      - run: cargo build\n        if: runner.os == 'Linux'\n",
            ),
        ),
        [],
    )
}

test "direct R2 setup requires actual mise tools and executed direnv commands" {
    let mise = "      - uses: ./.github/actions/setup-mise\n        with: {install_args: 'sccache direnv'}\n";
    let load = "      - run: |\n          direnv allow .envrc\n          direnv export gha >> \"$GITHUB_ENV\"\n";
    assert.equal(rules(workflow(mise + load + "      - run: cargo build\n")), []);
    assert.equal(rules(workflow(load + "      - run: cargo build\n")), ["RUST-SETUP"]);
    assert.equal(
        rules(
            workflow(
                mise
                    + "      - run: echo 'direnv allow .envrc; direnv export gha >> $GITHUB_ENV'\n      - run: cargo build\n",
            ),
        ),
        ["RUST-SETUP"],
    )
}

test "generic caches cannot smuggle Rust state through paths or expressions" {
    for (
        let path in [
            "target",
            "baml_language/target",
            "~/.cargo/registry",
            "~/.cache/sccache",
            "${{ inputs.path }}",
        ]
    ) {
        assert.equal(
            rules(workflow(`      - uses: actions/cache/restore@v6\n        with: {path: '${path}'}\n`)),
            ["RUST-CACHE"],
        );
    }
    assert.equal(
        rules(workflow("      - uses: actions/cache@v6\n        with: {path: '~/.cache/pre-commit'}\n")),
        [],
    )
}

test "backend and wrapper overrides are rejected at every env scope" {
    assert.equal(
        rules(workflow("      - run: echo ok\n        env: {SCCACHE_GHA_ENABLED: true}\n")),
        ["CACHE-CONFIG"],
    );
    assert.equal(
        rules("env: {RUSTC_WRAPPER: ''}\n" + workflow("      - run: echo ok\n")),
        ["CACHE-CONFIG"],
    )
}

test "canonical bootstrap exemptions do not cover unrelated installations" {
    let action = "name: setup\nruns:\n  using: composite\n  steps:\n    - uses: jdx/mise-action@v4\n    - run: uv tool install ruff\n";
    assert.is_true(
        rules(action, path = ".github/actions/setup-mise/action.yml").includes("TOOL-INSTALL"),
    );
    assert.is_true(
        rules(action, path = ".github/actions/fake-setup-mise/action.yml").includes("TOOL-ACTION"),
    );
    assert.is_true(
        rules(
            "name: cache\nruns:\n  using: composite\n  steps:\n    - run: cargo build -p unrelated\n",
            path = ".github/actions/setup-sccache/action.yml",
        )
            .includes("RUST-SETUP"),
    );
    assert.is_true(
        rules(
            "name: rust\nruns:\n  using: composite\n  steps: []\n",
            path = ".github/actions/setup-rust/action.yml",
        )
            .includes("BOOTSTRAP"),
    )
}

test "annotation text cannot inject workflow commands or attributes" {
    let d = Diagnostic {
        file: "file,evil\n::warning::oops",
        location: "step",
        rule: "RULE",
        message: "line1\n::notice::line2%",
    };
    let output = annotation(d);
    assert.equal(output.includes("\n"), false);
    assert.contains(output, "%2Cevil%0A%3A%3Awarning");
    assert.contains(output, "%25")
}

test "filesystem discovery includes new nested workflows and composite actions" {
    let root = `/tmp/baml-actions-discovery-${baml.sys.pid()}`;
    baml.fs.mkdir(`${root}/workflows/nested`, baml.fs.MkdirOptions { recursive: true });
    baml.fs.mkdir(`${root}/actions/future/nested`, baml.fs.MkdirOptions { recursive: true });
    defer { baml.fs.remove_dir_all(root); }
    baml.fs.write(`${root}/workflows/new.yaml`, workflow("      - uses: actions/setup-node@v7\n"));
    baml.fs.write(
        `${root}/workflows/nested/new.yml`,
        workflow("      - uses: Swatinem/rust-cache@v2\n"),
    );
    baml.fs.write(
        `${root}/actions/future/nested/action.yaml`,
        "name: x\nruns: {using: composite, steps: [{uses: astral-sh/setup-uv@v8}]}\n",
    );
    baml.fs.write(`${root}/actions/future/README.md`, "not an action");
    let files = discover(root, "workflows", true).concat(discover(root, "actions", false));
    assert.equal(files.length(), 3);
    assert.equal(lint_sources(files).diagnostics.length(), 3)
}

test "CLI checks untracked actions outside github and ignored future workflows and exits nonzero" {
    let root = `/tmp/baml-actions-cli-${baml.sys.pid()}`;
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    defer { baml.fs.remove_dir_all(root); }
    let opts = baml.sys.ProcessOptions { cwd: null, env: null, timeout_ms: 30000, stdin: null };
    let repo = baml.sys.exec("git", ["rev-parse", "--show-toplevel"], opts);
    assert.is_true(repo.ok());
    let project = baml.String.from_utf8(repo.stdout).trim() + "/tools/baml-github-actions-rules";
    assert.is_true(baml.sys.exec("git", ["init", "--quiet", root], opts).ok());
    for (let path in v0_exclusions().keys()) {
        let parts = path.split("/");
        baml.fs.mkdir(
            `${root}/${parts.slice(0, parts.length() - 1).join("/")}`,
            baml.fs.MkdirOptions { recursive: true },
        );
        baml.fs.write(`${root}/${path}`, "intentionally invalid excluded v0 fixture: [");
    }
    baml.fs.write(`${root}/.github/workflows/good.yaml`, workflow("      - run: echo ready\n"));
    let good = baml.sys.exec("baml", ["--project", project, "run", "main", "--", "--repo_root", root], opts);
    if (!good.ok()) {
        throw baml.errors.InvalidArgument { message: baml.String.from_utf8(good.stderr) + baml.String.from_utf8(good.stdout) };
    }
    assert.contains(baml.String.from_utf8(good.stdout), "0 violations");
    baml.fs.mkdir(`${root}/tools/future`, baml.fs.MkdirOptions { recursive: true });
    baml.fs.write(
        `${root}/tools/future/action.yaml`,
        "name: future\nruns: {using: composite, steps: [{uses: astral-sh/setup-uv@v8}]}\n",
    );
    baml.fs.write(`${root}/.gitignore`, ".github/workflows/future.yml\n");
    baml.fs.write(
        `${root}/.github/workflows/future.yml`,
        workflow("      - uses: Swatinem/rust-cache@v2\n"),
    );
    let bad = baml.sys.exec("baml", ["--project", project, "run", "main", "--", "--repo_root", root], opts);
    assert.equal(bad.exit_code, 1);
    let errors = baml.String.from_utf8(bad.stderr);
    assert.contains(errors, "::error file=tools/future/action.yaml,title=TOOL-ACTION");
    assert.contains(errors, "::error file=.github/workflows/future.yml,title=RUST-CACHE");
    assert.contains(baml.String.from_utf8(bad.stdout), "2 violations")
}

test "repository audit checks missing local actions and R2 forwarding" {
    let ds: Diagnostic[] = [];
    audit_repository(
        [
            Source {
                path: ".github/workflows/new.yml",
                text: workflow("      - uses: ./tools/missing\n"),
            },
        ],
        ds,
    );
    assert.equal(ds[0].rule, "LOCAL-ACTION");
    let cache = Source {
        path: ".github/actions/setup-sccache/action.yml",
        text: "runs: {using: composite, steps: []}",
    };
    let ds2: Diagnostic[] = [];
    audit_repository(
        [
            cache,
            Source {
                path: ".github/workflows/new.yml",
                text: workflow("      - uses: ./.github/actions/setup-sccache\n"),
            },
        ],
        ds2,
    );
    assert.equal(
        ds2.map((d) -> {
            d.rule
        }),
        ["R2-CREDENTIALS", "R2-CREDENTIALS"],
    );
    let receiver = Source {
        path: ".github/workflows/reusable.yml",
        text: "on: {workflow_call: {secrets: {BAML_SCCACHE_R2_ACCESS_KEY_ID: {required: false}, BAML_SCCACHE_R2_SECRET_ACCESS_KEY: {required: false}}}}\njobs: {build: {steps: [{run: 'echo ok'}]}}",
    };
    let ds3: Diagnostic[] = [];
    audit_repository(
        [
            receiver,
            Source {
                path: ".github/workflows/new.yml",
                text: "jobs: {call: {uses: './.github/workflows/reusable.yml'}}",
            },
        ],
        ds3,
    );
    assert.equal(
        ds3.map((d) -> {
            d.rule
        }),
        ["R2-CREDENTIALS"],
    )
}

test "canonical mise bootstrap settings and task auto-install cannot drift" {
    let config = "name: mise\nruns:\n  using: composite\n  steps:\n    - uses: jdx/mise-action@v4\n      with:\n        add_shims_to_path: true\n        cache_save: true\n        install_args: ''\n        cache_key_prefix: wrong\n";
    assert.equal(
        rules(config, path = ".github/actions/setup-mise/action.yml"),
        ["BOOTSTRAP", "BOOTSTRAP"],
    )
}
