// Exact files only. New files (including similarly named v0 files) are checked.
// Reasons record manually inspected build roots and call sites, not name guesses.
function v0_exclusions() -> map<string, string> {
    {
        ".github/workflows/primary.yml": "Legacy runtime CI builds engine/, typescript/, and integ-tests/ and synchronizes engine/zed.",
        ".github/workflows/release.yml": "Legacy version-tag release orchestrates engine CLI/SDKs, typescript VS Code, JetBrains and engine/zed publication.",
        ".github/workflows/build-cli-release.reusable.yaml": "Builds engine baml_cffi and the legacy baml-cli for release.yml.",
        ".github/workflows/build-python-release.reusable.yaml": "Builds engine/language_client_python wheels and audits those wheels.",
        ".github/workflows/build-ruby-release.reusable.yaml": "Builds engine/language_client_ruby gems with rb-sys-dock.",
        ".github/workflows/build-typescript-release.reusable.yaml": "Builds native addons in engine/language_client_typescript.",
        ".github/workflows/build-vscode-release.reusable.yaml": "Packages typescript/apps/vscode-ext with engine CLI artifacts and the legacy playground.",
        ".github/workflows/build-jetbrains-release.reusable.yaml": "Builds and verifies jetbrains/ for the legacy release.yml pipeline.",
        ".github/workflows/publish-jetbrains-0.225.1.yml": "One-off recovery publication of the legacy JetBrains 0.225.1 plugin.",
        ".github/workflows/publish-crates-0.226.2.yml": "One-off publication of languages/rust crates from verified engine release source.",
        ".github/workflows/publish-zed-release.reusable.yaml": "Publishes and synchronizes the engine/zed extension.",
        ".github/workflows/integ-tests.yml": "Runs legacy tools/bctl integration suites against engine clients.",
        ".github/workflows/test-go-windows-quick.yml": "Builds engine baml_cffi and tests engine/language_client_go on Windows.",
        ".github/workflows/test-go-windows.yml": "Tests engine/language_client_go against engine baml_cffi, including cross compilation.",
        ".github/workflows/test-rust-sdk.yml": "Builds engine baml_cffi and tests languages/rust (not baml_language/sdks/rust).",
        ".github/workflows/rust-coverage.yml": "Manual coverage of the engine workspace with legacy Go/Python/Ruby setup.",
        ".github/workflows/docs.reusable.yaml": "Shared Fern publisher called by ci.yaml; fern/ still publishes v0 documentation, so the whole existing file is explicitly excluded.",
        ".github/actions/engine-setup-rust/action.yml": "Rust/wasm/cross bootstrap and cache for engine release and runtime CI.",
        ".github/actions/engine-setup-node/action.yml": "Node/pnpm and Turbo setup for the legacy root workspace and engine clients.",
        ".github/actions/engine-setup-ruby/action.yml": "Ruby setup used by primary.yml and rust-coverage.yml for engine.",
        ".github/actions/setup-all/action.yml": "Legacy development composite defaults to engine and delegates to engine setup actions.",
        ".github/actions/setup-go/action.yml": "Shared legacy Go setup used by engine CLI, runtime and integration workflows.",
        ".github/actions/setup-java/action.yml": "Java/Gradle setup used by legacy JetBrains release and recovery workflows.",
        ".github/actions/setup-node/action.yml": "Legacy root workspace Node/pnpm/Turbo setup; v1 callers must migrate to mise.",
        ".github/actions/setup-protoc/action.yml": "Shared protoc bootstrap used by engine-setup-rust and legacy Rust SDK tests; v1 callers must use mise.",
        ".github/actions/setup-python/action.yml": "Legacy Python/uv/maturin/ruff setup with integ-tests/python dependency installation.",
        ".github/actions/setup-tools/action.yml": "Legacy disk cleanup and direct mise bootstrap used by engine workflows.",
    }
}
