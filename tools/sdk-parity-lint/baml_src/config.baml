function sdk_environments() -> string[] {
    [
        "python_pydantic2",
        "typescript_node",
        "typescript_web_chromium",
        "typescript_web_cloudflare_workers",
        "cpp",
        "csharp",
        "rust",
        "go",
        "java",
        "swift",
    ]
}

function make_source_config(
    language: string,
    source_directory: string,
    source_root: string,
    rule_file: string,
    file_glob: string,
    suffix: string,
    declaration_rule_ids: string[],
    raw_name_field: string?,
    prefix: string,
) -> SourceConfig {
    SourceConfig {
        language: language,
        source_directory: source_directory,
        source_root: source_root,
        rule_file: rule_file,
        file_glob: file_glob,
        suffix: suffix,
        argv: [
            "exec",
            "ast-grep",
            "--",
            "ast-grep",
            "scan",
            "--rule",
            rule_file,
            "--json=stream",
            "--color",
            "never",
            "--no-ignore=vcs",
            "--globs",
            file_glob,
            source_root,
        ],
        declaration_rule_ids: declaration_rule_ids,
        raw_name_field: raw_name_field,
        prefix: prefix,
    }
}

function source_configs(repo_root: string) -> SourceConfig[] {
    let rules_root = `${repo_root}/tools/sdk-parity-lint/ast-grep-rules`;
    [
        make_source_config(
            "python",
            "python_pydantic2",
            "baml_language/sdk_tests/crates/python_pydantic2",
            `${rules_root}/python.yml`,
            "**/customizable/**/*.py",
            ".py",
            ["python-test"],
            "NAME",
            "test_",
        ),
        make_source_config(
            "typescript",
            "typescript",
            "baml_language/sdk_tests/crates/typescript",
            `${rules_root}/typescript.yml`,
            "**/customizable/**/*.ts",
            ".ts",
            ["typescript-test", "typescript-modified-test", "typescript-runtime-test"],
            "ID",
            "",
        ),
        make_source_config(
            "cpp",
            "cpp",
            "baml_language/sdk_tests/crates/cpp",
            `${rules_root}/cpp.yml`,
            "**/customizable/**/*.cc",
            ".cc",
            ["cpp-test"],
            null,
            "",
        ),
        make_source_config(
            "csharp",
            "csharp",
            "baml_language/sdk_tests/crates/csharp",
            `${rules_root}/csharp.yml`,
            "**/src/**/*.rs",
            ".rs",
            ["csharp-test"],
            "NAME",
            "test_",
        ),
        make_source_config(
            "rust",
            "rust",
            "baml_language/sdk_tests/crates/rust",
            `${rules_root}/rust.yml`,
            "**/customizable/**/*.rs",
            ".rs",
            ["rust-test"],
            "NAME",
            "test_",
        ),
        make_source_config(
            "go",
            "go",
            "baml_language/sdk_tests/crates/go",
            `${rules_root}/go.yml`,
            "**/customizable/**/*_test.go",
            "_test.go",
            ["go-test"],
            "NAME",
            "Test_",
        ),
        make_source_config(
            "java",
            "java",
            "baml_language/sdk_tests/crates/java",
            `${rules_root}/java.yml`,
            "**/customizable/**/*.java",
            ".java",
            ["java-test"],
            "NAME",
            "test_",
        ),
        make_source_config(
            "swift",
            "swift",
            "baml_language/sdk_tests/crates/swift",
            `${rules_root}/swift.yml`,
            "**/customizable/**/*.swift",
            ".swift",
            ["swift-test"],
            "NAME",
            "test_",
        ),
    ]
}

function source_config_for(configs: SourceConfig[], source_directory: string) -> SourceConfig {
    for (let config in configs) {
        if (config.source_directory == source_directory) {
            return config;
        }
    }
    throw baml.errors.InvalidArgument {
        message: `missing source configuration for ${source_directory}`,
    };
}

function sdk_source_directory(sdk_env: string) -> string {
    if (sdk_env.starts_with("typescript_")) {
        "typescript"
    } else {
        sdk_env
    }
}

function sdk_runtime(sdk_env: string) -> string? {
    if (sdk_env == "typescript_node") {
        "node"
    } else if (sdk_env == "typescript_web_chromium") {
        "web"
    } else if (sdk_env == "typescript_web_cloudflare_workers") {
        "workers"
    } else {
        null
    }
}

function resolve_output_dir(repo_root: string, output_dir: string) -> string {
    if (output_dir.starts_with("/")) {
        trim_trailing_slashes(output_dir)
    } else {
        `${trim_trailing_slashes(repo_root)}/${trim_surrounding_slashes(output_dir)}`
    }
}

function trim_trailing_slashes(value: string) -> string {
    let end = value.length();
    while (end > 1 && value.char_at(end - 1) == "/") {
        end -= 1;
    }
    value.substring(0, end)
}

function trim_surrounding_slashes(value: string) -> string {
    let start = 0;
    let end = value.length();
    while (start < end && value.char_at(start) == "/") {
        start += 1;
    }
    while (end > start && value.char_at(end - 1) == "/") {
        end -= 1;
    }
    value.substring(start, end)
}
