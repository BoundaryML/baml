function array_contains(values: string[], expected: string) -> bool {
    values
        .some(
            (value) -> {
                value == expected
            },
        )
}

function is_canonical_component(value: string) -> bool {
    if (value.length() == 0) {
        return false;
    }
    let lowercase = "abcdefghijklmnopqrstuvwxyz";
    let allowed = "abcdefghijklmnopqrstuvwxyz0123456789_";
    if (!lowercase.matches(value.char_at(0))) {
        return false;
    }
    let index = 0;
    while (index < value.length()) {
        if (!allowed.matches(value.char_at(index))) {
            return false;
        }
        index += 1;
    }
    true
}

function hex_digit_value(character: string) -> int {
    let lowercase = "0123456789abcdef";
    let uppercase = "0123456789ABCDEF";
    let lower_index = lowercase.index_of(character);
    if let value: int = lower_index {
        value
    } else if let value: int = uppercase.index_of(character) {
        value
    } else {
        -1
    }
}

function parse_hex_digits(raw_name: string, start: int, count: int, location: string) -> int {
    if (start + count > raw_name.length()) {
        throw baml.errors.ParseError {
            message: `invalid hexadecimal escape in TypeScript test title at ${location}`,
        };
    }
    let value = 0;
    let index = start;
    while (index < start + count) {
        let digit = hex_digit_value(raw_name.char_at(index));
        if (digit < 0) {
            throw baml.errors.ParseError {
                message: `invalid hexadecimal escape in TypeScript test title at ${location}`,
            };
        }
        value = value * 16 + digit;
        index += 1;
    }
    value
}

function decode_typescript_id(raw_name: string, source_file: string, source_line: int) -> string {
    let location = `${source_file}:${source_line}`;
    if (raw_name.length() < 2) {
        throw baml.errors.ParseError {
            message: `TypeScript test title at ${location} is not a static quoted string: ${raw_name}`,
        };
    }
    let first = raw_name.char_at(0);
    let last = raw_name.char_at(raw_name.length() - 1);
    if (!((first == "\"" && last == "\"") || (first == "'" && last == "'"))) {
        throw baml.errors.ParseError {
            message: `TypeScript test title at ${location} is not a static quoted string: ${raw_name}`,
        };
    }
    let code_points: int[] = [];
    let index = 1;
    while (index < raw_name.length() - 1) {
        let character = raw_name.char_at(index);
        if (character != "\\") {
            if (character == "\n" || character == "\r") {
                throw baml.errors.ParseError {
                    message: `multiline TypeScript test title at ${location}`,
                };
            }
            code_points.push(character.code_point_at(0));
            index += 1;
        } else {
            index += 1;
            if (index >= raw_name.length() - 1) {
                throw baml.errors.ParseError {
                    message: `unterminated escape in TypeScript test title at ${location}`,
                };
            }
            let escape = raw_name.char_at(index);
            index += 1;
            if (escape == "'" || escape == "\"" || escape == "\\") {
                code_points.push(escape.code_point_at(0));
            } else if (escape == "b") {
                code_points.push(8);
            } else if (escape == "f") {
                code_points.push(12);
            } else if (escape == "n") {
                code_points.push(10);
            } else if (escape == "r") {
                code_points.push(13);
            } else if (escape == "t") {
                code_points.push(9);
            } else if (escape == "v") {
                code_points.push(11);
            } else if (escape == "0") {
                if (index < raw_name.length() - 1 && raw_name.char_at(index).is_numeric()) {
                    throw baml.errors.ParseError {
                        message: `legacy octal escape in TypeScript test title at ${location}`,
                    };
                }
                code_points.push(0);
            } else if (escape == "x") {
                code_points.push(parse_hex_digits(raw_name, index, 2, location));
                index += 2;
            } else if (escape == "u") {
                if (index < raw_name.length() - 1 && raw_name.char_at(index) == "{") {
                    index += 1;
                    let digits_start = index;
                    while (index < raw_name.length() - 1 && raw_name.char_at(index) != "}") {
                        index += 1;
                    }
                    let count = index - digits_start;
                    if (index >= raw_name.length() - 1 || count < 1 || count > 6) {
                        throw baml.errors.ParseError {
                            message: `invalid Unicode escape in TypeScript test title at ${location}`,
                        };
                    }
                    let code_point = parse_hex_digits(raw_name, digits_start, count, location);
                    if (code_point > 1114111) {
                        throw baml.errors.ParseError {
                            message: `invalid Unicode escape in TypeScript test title at ${location}`,
                        };
                    }
                    code_points.push(code_point);
                    index += 1;
                } else {
                    let code_point = parse_hex_digits(raw_name, index, 4, location);
                    index += 4;
                    if (
                        code_point >= 55296
                            && code_point <= 56319
                            && index + 6 <= raw_name.length() - 1
                            && raw_name.substring(index, index + 2) == "\\u"
                    ) {
                        let low = parse_hex_digits(raw_name, index + 2, 4, location);
                        if (low >= 56320 && low <= 57343) {
                            code_point = 65536 + (code_point - 55296) * 1024 + low - 56320;
                            index += 6;
                        }
                    }
                    if (code_point >= 55296 && code_point <= 57343) {
                        throw baml.errors.ParseError {
                            message: `invalid Unicode escape in TypeScript test title at ${location}`,
                        };
                    }
                    code_points.push(code_point);
                }
            } else if (escape == "\n") {
                null
            } else if (escape == "\r") {
                if (index < raw_name.length() - 1 && raw_name.char_at(index) == "\n") {
                    index += 1;
                }
            } else {
                code_points.push(escape.code_point_at(0));
            }
        }
    }
    string.from_code_points(code_points)
}

function normalize_raw_name(
    config: SourceConfig,
    raw_name: string,
    source_file: string,
    source_line: int,
) -> string {
    let normalized = if (config.language == "typescript") {
        decode_typescript_id(raw_name, source_file, source_line)
    } else if (config.prefix.length() == 0) {
        raw_name
    } else if (raw_name.starts_with(config.prefix)) {
        raw_name.substring(config.prefix.length(), raw_name.length())
    } else {
        throw baml.errors.ParseError {
            message: `${config.language} test declaration at ${source_file}:${source_line} must start with '${config.prefix}', got ${raw_name}`,
        };
    };
    if (!is_canonical_component(normalized)) {
        throw baml.errors.ParseError {
            message: `test ID component at ${source_file}:${source_line} must match ^[a-z][a-z0-9_]*$, got ${normalized}`,
        };
    }
    normalized
}

function category_for(config: SourceConfig, source_file: string) -> string {
    let prefix = `${config.source_root}/`;
    if (!source_file.starts_with(prefix)) {
        throw baml.errors.InvalidArgument {
            message: `${source_file} is outside configured source root ${config.source_root}`,
        };
    }
    let remainder = source_file.substring(prefix.length(), source_file.length());
    if (config.source_directory == "csharp") {
        return "integration";
    }
    let parts = remainder.split("/");
    let inside_customizable = false;
    let index = 1;
    while (index < parts.length() - 1) {
        if (parts[index] == "customizable") {
            inside_customizable = true;
        }
        index += 1;
    }
    if (parts.length() < 3 || !inside_customizable) {
        throw baml.errors.InvalidArgument {
            message: `${source_file} is not inside <category>/customizable`,
        };
    }
    let category = parts[0];
    if (!is_canonical_component(category)) {
        throw baml.errors.InvalidArgument {
            message: `source category in ${source_file} is not canonical: ${category}`,
        };
    }
    category
}

function audit_rule_ids(language: string) -> string[] {
    if (language == "typescript") {
        [
            "typescript-audit-direct",
            "typescript-audit-modifier",
            "typescript-audit-curried",
            "typescript-audit-nested-modifier",
            "typescript-audit-nested-curried",
        ]
    } else if (
        language == "python"
            || language == "cpp"
            || language == "rust"
            || language == "go"
            || language == "java"
            || language == "swift"
            || language == "csharp"
    ) {
        [`${language}-audit-test`]
    } else {
        []
    }
}

function validate_audit_coverage(scan: SourceScan) -> void {
    let audit_ids = audit_rule_ids(scan.config.language);
    let accepted: map<string, bool> = {};
    for (let ast_match in scan.matches) {
        if (array_contains(scan.config.declaration_rule_ids, ast_match.ruleId)) {
            let _ = accepted.set(ast_range_key(ast_match), true);
        }
    }
    let raw_audits: AstMatch[] = [];
    for (let ast_match in scan.matches) {
        if (array_contains(audit_ids, ast_match.ruleId)) {
            raw_audits.push(ast_match);
        }
    }
    let audits: AstMatch[] = [];
    for (let audit in raw_audits) {
        let shadowed = false;
        for (let other in raw_audits) {
            if (
                other.file == audit.file
                    && other.range.byteOffset.start == audit.range.byteOffset.start
                    && other.range.byteOffset.end > audit.range.byteOffset.end
            ) {
                shadowed = true;
            }
        }
        if (!shadowed) {
            audits.push(audit);
        }
    }
    let audited: map<string, bool> = {};
    for (let audit in audits) {
        let key = ast_range_key(audit);
        let _ = audited.set(key, true);
        if (!accepted.has(key)) {
            throw baml.errors.ParseError {
                message: `unsupported ${scan.config.language} test declaration form at ${audit.file}:${audit.range.start.line + 1}: ${audit.text}\nSDK test names must use the parity lint's canonical form and match the corresponding test name across SDKs. If this behavior is intentionally SDK-specific, give the test a canonical name and add an immediately preceding SDK_PARITY_LINT(skip): <reason> directive.`,
            };
        }
    }
    for (let ast_match in scan.matches) {
        if (
            array_contains(scan.config.declaration_rule_ids, ast_match.ruleId)
                && !audited.has(ast_range_key(ast_match))
        ) {
            throw baml.errors.ParseError {
                message: `accepted ${scan.config.language} test was not covered by an audit rule at ${ast_match.file}:${ast_match.range.start.line + 1}`,
            };
        }
    }
}

function discovered_test_from_match(config: SourceConfig, ast_match: AstMatch) -> DiscoveredTest {
    let raw_value = if let field: string = config.raw_name_field {
        ast_single(ast_match, field)
    } else {
        AstMetaValue { text: ast_match.text, range: ast_match.range }
    };
    let name = normalize_raw_name(config, raw_value.text, ast_match.file, raw_value.range.start.line + 1);
    let category = category_for(config, ast_match.file);
    let runtime_condition = if (ast_match.ruleId == "typescript-runtime-test") {
        ast_single(ast_match, "COND").text
    } else {
        null
    };
    DiscoveredTest {
        language: config.language,
        source_directory: config.source_directory,
        sdk_test_id: `${category}/${name}`,
        raw_name: raw_value.text,
        source_file: ast_match.file,
        source_line: raw_value.range.start.line + 1,
        source_column: raw_value.range.start.column + 1,
        match_start: ast_match.range.byteOffset.start,
        match_end: ast_match.range.byteOffset.end,
        match_start_line: ast_match.range.start.line + 1,
        rule_id: ast_match.ruleId,
        runtime_condition: runtime_condition,
        runtimes: [],
    }
}

function discover_tests(scans: SourceScan[]) -> DiscoveredTest[] {
    let tests: DiscoveredTest[] = [];
    for (let scan in scans) {
        validate_audit_coverage(scan);
        for (let ast_match in scan.matches) {
            if (array_contains(scan.config.declaration_rule_ids, ast_match.ruleId)) {
                tests.push(discovered_test_from_match(scan.config, ast_match));
            }
        }
    }
    tests
        .sort_by_key(
            (test_case) -> {
                `${test_case.source_file}:${test_case.source_line}:${test_case.sdk_test_id}`
            },
        );
    tests
}
