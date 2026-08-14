function configured_ast_grep_version(repo_root: string) -> string {
    for (let line in baml.fs.read(`${repo_root}/mise.toml`).lines()) {
        let trimmed = line.trim();
        if (trimmed.starts_with("ast-grep = \"") && trimmed.ends_with("\"")) {
            return trimmed.substring(12, trimmed.length() - 1);
        }
    }
    throw baml.errors.InvalidArgument {
        message: "mise.toml does not pin ast-grep",
    };
}

function verify_ast_grep(repo_root: string) -> string {
    let expected = configured_ast_grep_version(repo_root);
    let result = baml
        .sys
        .exec(
            "mise",
            ["exec", "ast-grep", "--", "ast-grep", "--version"],
            baml.sys.ProcessOptions { cwd: repo_root, env: null, timeout_ms: 120000, stdin: null },
        );
    if (!result.ok()) {
        throw baml.errors.Io {
            message: `mise could not run ast-grep: ${baml.String.from_utf8(result.stderr)}`,
        };
    }
    let actual = baml.String.from_utf8(result.stdout).trim();
    if (actual != `ast-grep ${expected}`) {
        throw baml.errors.InvalidArgument {
            message: `mise.toml pins ast-grep ${expected}, but mise ran ${actual}`,
        };
    }
    `ast-grep ${expected}`
}

function owned_string(value: string) -> string {
    baml.String.from_utf8(value.to_utf8())
}

function owned_ast_position(position: AstPosition) -> AstPosition {
    AstPosition { line: position.line, column: position.column }
}

function owned_ast_range(range: AstRange) -> AstRange {
    AstRange {
        byteOffset: AstByteOffset { start: range.byteOffset.start, end: range.byteOffset.end },
        start: owned_ast_position(range.start),
        end: owned_ast_position(range.end),
    }
}

function owned_ast_meta_value(value: AstMetaValue) -> AstMetaValue {
    AstMetaValue { text: owned_string(value.text), range: owned_ast_range(value.range) }
}

function owned_ast_meta_variables(meta_variables: AstMetaVariables?) -> AstMetaVariables? {
    if let present: AstMetaVariables = meta_variables {
        let single: map<string, AstMetaValue> = {};
        for (let name in present.single.keys()) {
            if let value: AstMetaValue = present.single.get(name) {
                let _ = single.set(owned_string(name), owned_ast_meta_value(value));
            }
        }
        AstMetaVariables { single: single }
    } else {
        null
    }
}

function owned_ast_match(ast_match: AstMatch) -> AstMatch {
    AstMatch {
        text: owned_string(ast_match.text),
        file: owned_string(ast_match.file),
        range: owned_ast_range(ast_match.range),
        metaVariables: owned_ast_meta_variables(ast_match.metaVariables),
        ruleId: owned_string(ast_match.ruleId),
    }
}

function parse_ast_grep_stream(stdout: uint8array, config: SourceConfig) -> AstMatch[] {
    let matches: AstMatch[] = [];
    for (let line in baml.String.from_utf8(stdout).lines()) {
        if (line.trim().length() > 0) {
            let decoded: AstMatch = baml.json.from_string<AstMatch>(line);
            let ast_match = owned_ast_match(decoded);
            if (!ast_match.file.starts_with(`${config.source_root}/`)) {
                throw baml.errors.InvalidArgument {
                    message: `ast-grep returned ${ast_match.file} outside ${config.source_root}`,
                };
            }
            let _ = category_for(config, ast_match.file);
            if (!ast_match.file.ends_with(config.suffix)) {
                throw baml.errors.InvalidArgument {
                    message: `ast-grep returned an unexpected file type for ${config.language}: ${ast_match.file}`,
                };
            }
            matches.push(ast_match);
        }
    }
    matches
        .sort_by_key(
            (ast_match) -> {
                `${ast_match.file}:${ast_match.range.byteOffset.start}:${ast_match.ruleId}`
            },
        );
    matches
}

function scan_source(repo_root: string, config: SourceConfig) -> SourceScan {
    if (!baml.fs.exists(config.rule_file)) {
        throw baml.errors.Io {
            message: `missing ast-grep rule file ${config.rule_file}`,
        };
    }
    let result = baml
        .sys
        .exec(
            "mise",
            config.argv,
            baml.sys.ProcessOptions { cwd: repo_root, env: null, timeout_ms: 120000, stdin: null },
        );
    if (!result.ok()) {
        throw baml.errors.Io {
            message: `ast-grep failed for ${config.language}: ${baml.String.from_utf8(result.stderr)}`,
        };
    }
    SourceScan { config: config, matches: parse_ast_grep_stream(result.stdout, config) }
}

function scan_all_sources(repo_root: string, configs: SourceConfig[]) -> SourceScan[] {
    if (configs.length() != 8) {
        throw baml.errors.InvalidArgument {
            message: `expected eight SDK source configurations, got ${configs.length()}`,
        };
    }
    let python = spawn { scan_source(repo_root, configs[0]) };
    let typescript = spawn { scan_source(repo_root, configs[1]) };
    let cpp = spawn { scan_source(repo_root, configs[2]) };
    let csharp = spawn { scan_source(repo_root, configs[3]) };
    let rust = spawn { scan_source(repo_root, configs[4]) };
    let go = spawn { scan_source(repo_root, configs[5]) };
    let java = spawn { scan_source(repo_root, configs[6]) };
    let swift = spawn { scan_source(repo_root, configs[7]) };
    [
        await python,
        await typescript,
        await cpp,
        await csharp,
        await rust,
        await go,
        await java,
        await swift,
    ]
}

function ast_single(ast_match: AstMatch, name: string) -> AstMetaValue {
    if let meta_variables: AstMetaVariables = ast_match.metaVariables {
        if let value: AstMetaValue = meta_variables.single.get(name) {
            return value;
        }
    }
    throw baml.errors.ParseError {
        message: `ast-grep rule ${ast_match.ruleId} did not bind $${name} at ${ast_match.file}:${ast_match.range.start.line + 1}`,
    };
}

function ast_range_key(ast_match: AstMatch) -> string {
    `${ast_match.file}:${ast_match.range.byteOffset.start}:${ast_match.range.byteOffset.end}`
}
