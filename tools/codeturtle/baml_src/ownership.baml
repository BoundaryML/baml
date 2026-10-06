function words(text: string) -> string[] {
    let out: string[] = [];
    for (let token in text.replace_all("\t", " ").split(" ")) {
        if (token != "") {
            out.push(token);
        }
    }
    out
}

// Small syntax subset: '*', exact rooted paths, rooted directory prefixes,
// optional 'all', and function-name conditions. Prompt conditions are rejected.
function parse_rules(source: string) -> Rule[] {
    let rules: Rule[] = [];
    let line_number = 0;
    for (let source_line in source.lines()) {
        line_number += 1;
        let line = source_line.trim();
        if (line == "" || line.starts_with("#")) {
            continue;
        }
        let parts = line.split("#");
        let tokens = words(parts[0].trim());
        if (tokens.length() < 2) {
            throw "An ownership rule requires a path and owners";
        }
        let owners: string[] = [];
        for (let token in tokens.slice(1, tokens.length())) {
            if (!token.starts_with("@")) {
                throw "Owners must start with @";
            }
            owners.push(token);
        }
        let approval: Approval = Approval.Any;
        let condition: string? = null;
        if (parts.length() > 1) {
            let directive = parts[1].trim();
            if (directive.starts_with("codeturtle:")) {
                for (let raw in directive.slice("codeturtle:".length(), directive.length()).split(",")) {
                    let option = raw.trim();
                    if (option == "all") {
                        approval = Approval.All;
                    } else if (option.starts_with("when=")) {
                        let name = option.slice(5, option.length()).trim();
                        if (name == "" || name.includes("\"") || name.includes("'")) {
                            throw "This experiment supports function conditions only";
                        }
                        condition = name;
                    } else {
                        throw "Unknown codeturtle rule modifier";
                    }
                }
            }
        }
        if (
            tokens[0] != "*"
                && (
                    !tokens[0].starts_with("/")
                        || tokens[0].includes("*")
                        || tokens[0].includes("?")
                        || tokens[0].includes("[")
                )
        ) {
            throw "This experiment supports '*', rooted exact paths, and rooted directory prefixes";
        }
        rules.push(
            Rule {
                line: line_number,
                pattern: tokens[0],
                owners: owners,
                approval: approval,
                condition: condition,
            },
        );
    }
    rules
}

function matches_path(pattern: string, path: string) -> bool {
    if (pattern == "*") {
        return true;
    }
    let rooted = "/" + path;
    if (pattern.ends_with("/")) {
        rooted.starts_with(pattern)
    } else {
        rooted == pattern
    }
}
