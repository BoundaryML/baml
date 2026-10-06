function lint_sources(owners: string, sources: map<string, string>) -> LintReport {
    let report = LintReport { valid: false, rule_count: 0, source_count: 0, conditions: [], errors: [] };
    let rules = parse_rules(owners) catch_all (error) {
        _ => {
            report.errors.push(EvaluationError { rule_line: null, message: error.to_string() });
            return report;
        }
    };
    report.rule_count = rules.length();
    if (rules.length() == 0) {
        report.errors.push(
            EvaluationError { rule_line: null, message: "Ownership policy contains no rules" },
        );
    }
    report.source_count = sources.keys().length();
    let package = reflect.Package.compile(sources, packages = { "codeturtle": reflect.Package.current() }) catch_all (error) {
        _ => {
            report.errors.push(EvaluationError { rule_line: null, message: error.to_string() });
            return report;
        }
    };
    let seen: string[] = [];
    for (let rule in rules) {
        if (rule.condition == null) {
            continue;
        }
        let reference = rule.condition ?? "";
        let scope = condition_scope(reference, package) catch_all (error) {
            _ => {
                report.errors.push(
                    EvaluationError {
                        rule_line: rule.line,
                        message: "Invalid condition: " + reference + ": " + error.to_string(),
                    },
                );
                continue;
            }
        };
        if (!seen.includes(reference)) {
            seen.push(reference);
            report.conditions.push(ConditionInfo { name: reference, scope: scope });
        }
    }
    report.valid = report.errors.length() == 0;
    report
}
