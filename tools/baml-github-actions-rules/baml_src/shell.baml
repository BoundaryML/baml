// Lex executable command words, preserving quoted arguments and dropping comments.
// This is deliberately a convention checker, not a shell interpreter. It does not
// execute scripts or infer arbitrary variable values. Command substitutions are
// scanned separately so an installer inside $(...) cannot hide in an assignment.
class HereDocument {
    delimiter: string,
    expand: bool,
    strip_tabs: bool,
    executable: bool,
}

// Match the closing parenthesis without counting literal parentheses in quotes.
function substitution_end(script: string, start: int) -> int {
    let depth = 1;
    let quote = "";
    let escaped = false;
    let i = start;
    while (i < script.length()) {
        let c = script.at(i) ?? "";
        if (escaped) {
            escaped = false;
        } else if (c == "\\" && quote != "'") {
            escaped = true;
        } else if (quote != "") {
            if (c == quote) {
                quote = "";
            }
        } else if (c == "'" || c == "\"") {
            quote = c;
        } else if (c == "(") {
            depth += 1;
        } else if (c == ")") {
            depth -= 1;
            if (depth == 0) {
                return i;
            }
        }
        i += 1;
    }
    i
}

function shell_commands(script: string, substitutions_only: bool = false) -> string[][] {
    let commands: string[][] = [];
    let words: string[] = [];
    let word = "";
    let quote = "";
    let comment = false;
    let escaped = false;
    let i = 0;
    let here_documents: HereDocument[] = [];
    while (i < script.length()) {
        let c = script.at(i) ?? "";
        if (c == "\n" && quote == "" && !escaped && here_documents.length() > 0) {
            if (word != "") {
                words.push(word);
                word = "";
            }
            if (words.length() > 0) {
                commands.push(words);
                words = [];
            }
            comment = false;
            i += 1;
            for (let document in here_documents) {
                let body = "";
                while (i < script.length()) {
                    let end = i + (script.slice(i, script.length()).index_of("\n") ?? (script.length() - i));
                    let line = script.slice(i, end);
                    if (line.ends_with("\r")) {
                        line = line.slice(0, line.length() - 1);
                    }
                    if (document.strip_tabs) {
                        while (line.starts_with("\t")) {
                            line = line.slice(1, line.length());
                        }
                    }
                    i = end + 1;
                    if (line == document.delimiter) {
                        break;
                    }
                    body += line + "\n";
                }
                if (document.executable) {
                    commands = commands.concat(shell_commands(body));
                } else if (document.expand) {
                    commands = commands.concat(shell_commands(body, substitutions_only = true));
                }
            }
            here_documents = [];
            continue;
        }
        if (substitutions_only && !(c == "$" && script.at(i + 1) == "(") && c != "`") {
            i += 1;
            continue;
        }
        if (comment) {
            if (c == "\n") {
                comment = false;
                if (word != "") {
                    words.push(word);
                    word = "";
                }
                if (words.length() > 0) {
                    commands.push(words);
                    words = [];
                }
            }
        } else if (escaped) {
            if (c != "\n" && c != "\r") {
                word += c;
            }
            escaped = false;
        } else if (c == "\\" && quote != "'") {
            escaped = true;
        } else if (c == "$" && script.at(i + 1) == "(" && quote != "'") {
            let start = i + 2;
            i = substitution_end(script, start);
            commands = commands.concat(shell_commands(script.slice(start, i)));
            word += "<substitution>";
        } else if (c == "`" && quote != "'") {
            let start = i + 1;
            i += 1;
            while (i < script.length() && script.at(i) != "`") {
                if (script.at(i) == "\\") {
                    i += 1;
                }
                i += 1;
            }
            commands = commands.concat(shell_commands(script.slice(start, i)));
            word += "<substitution>";
        } else if (quote != "") {
            if (c == quote) {
                quote = "";
            } else {
                word += c;
            }
        } else if (c == "'" || c == "\"") {
            quote = c;
        } else if (c == "<" && script.at(i + 1) == "<" && script.at(i + 2) != "<") {
            if (word != "") {
                words.push(word);
                word = "";
            }
            i += 2;
            let strip_tabs = script.at(i) == "-";
            if (strip_tabs) {
                i += 1;
            }
            while (script.at(i) == " " || script.at(i) == "\t") {
                i += 1;
            }
            let delimiter = "";
            let delimiter_quote = "";
            let expand = true;
            while (i < script.length()) {
                let d = script.at(i) ?? "";
                if (delimiter_quote != "") {
                    if (d == delimiter_quote) {
                        delimiter_quote = "";
                    } else {
                        delimiter += d;
                    }
                } else if (d == "'" || d == "\"") {
                    delimiter_quote = d;
                    expand = false;
                } else if (d == "\\") {
                    expand = false;
                    i += 1;
                    delimiter += script.at(i) ?? "";
                } else if (d.is_whitespace() || [";", "|", "&", ")"].includes(d)) {
                    break;
                } else {
                    delimiter += d;
                }
                i += 1;
            }
            here_documents.push(
                HereDocument {
                    delimiter,
                    expand,
                    strip_tabs,
                    executable: ["bash", "sh", "zsh"].includes(executable_words(words).at(0) ?? ""),
                },
            );
            i -= 1;
        } else if (c == "#" && word == "") {
            comment = true;
        } else if (c == "\n" || c == ";" || c == "|" || c == "&" || c == "(" || c == ")") {
            if (word != "") {
                words.push(word);
                word = "";
            }
            if (words.length() > 0) {
                commands.push(words);
                words = [];
            }
        } else if (c.is_whitespace()) {
            if (word != "") {
                words.push(word);
                word = "";
            }
        } else {
            word += c;
        }
        i += 1;
    }
    if (word != "") {
        words.push(word);
    }
    if (words.length() > 0) {
        commands.push(words);
    }
    commands
}

function executable_words(words: string[]) -> string[] {
    let i = 0;
    while (i < words.length()) {
        let w = words[i];
        if (
            [
                "{",
                "}",
                "if",
                "then",
                "elif",
                "else",
                "do",
                "!",
                "sudo",
                "env",
                "command",
                "exec",
                "nohup",
                "time",
            ]
                .includes(w)
                || w.starts_with("-")
                || (w.includes("=") && !w.starts_with("="))
        ) {
            i += 1;
        } else if (w == "timeout") {
            i += 2;
        } else {
            break;
        }
    }
    let result = words.slice(i, words.length());
    if (result.length() > 0) {
        result[0] = result[0].replace_all("\\", "/").split("/").at(-1) ?? result[0];
        result[0] = result[0].to_lower_case();
    }
    result
}

function installs_tool(words: string[]) -> bool {
    let w = executable_words(words);
    let exe = w.at(0) ?? "";
    let args = w.slice(1, w.length());
    if (["echo", "printf", "write-output", "write-host", "test", "[", "[["].includes(exe)) {
        return false;
    }
    if (
        ["bash", "sh", "pwsh", "powershell"].includes(exe)
            && (args.includes("-c") || args.includes("-command"))
    ) {
        return args.some((arg) -> {
            shell_commands(arg).some((cmd) -> {
                installs_tool(cmd)
            })
        });
    }
    if (
        ["curl", "wget", "invoke-webrequest", "invoke-restmethod", "iwr", "irm"].includes(exe)
            && args.some((a) -> {
                (
                    a.includes("install.sh")
                        || a.includes("install.ps1")
                        || a.includes("get.pnpm.io")
                        || a.includes("mise.run")
                )
                    && !a.includes("sh.rustup.rs")
            })
    ) {
        return true;
    }
    if (exe == "baml" && args.includes("toolchain") && args.includes("use")) {
        return true;
    }
    if (
        ["apt", "apt-get", "brew", "choco", "winget"].includes(exe)
            && args.includes("install")
            && args.some((a) -> {
                [
                    "node",
                    "nodejs",
                    "npm",
                    "pnpm",
                    "python",
                    "python3",
                    "uv",
                    "ruby",
                    "go",
                    "golang",
                    "rust",
                    "cargo",
                    "rustup",
                    "mise",
                    "sccache",
                    "dotnet",
                    "clang",
                    "llvm",
                ]
                    .some((tool) -> {
                        a == tool || a.starts_with(tool + "-") || a.starts_with(tool + "@")
                    })
            })
    ) {
        return true;
    }
    if (exe == "cargo" && (args.includes("install") || args.includes("binstall"))) {
        return true;
    }
    if (exe == "mise" && (args.includes("install") || args.includes("use") || args.includes("i"))) {
        return true;
    }
    if (
        exe == "uv"
            && (
                (args.includes("tool") && args.includes("install"))
                    || (args.includes("python") && args.includes("install"))
            )
    ) {
        return true;
    }
    if (
        ["npm", "pnpm", "yarn"].includes(exe)
            && (args.includes("install") || args.includes("i") || args.includes("add"))
            && (
                args.some((a) -> {
                    a.starts_with("-") && !a.starts_with("--") && a.includes("g")
                })
                    || args.includes("--global")
                    || args.includes("global")
                    || args.some((a) -> {
                        a.starts_with("--global=")
                    })
            )
    ) {
        return true;
    }
    if (
        exe == "corepack"
            && (args.includes("enable") || args.includes("prepare") || args.includes("install"))
    ) {
        return true;
    }
    if (exe == "go" && args.includes("install")) {
        return true;
    }
    if (exe == "pipx" && args.includes("install")) {
        return true;
    }
    if (
        ["pip", "pip3", "python", "python3", "uv"].includes(exe)
            && args.includes("install")
            && args.some((a) -> {
                ["uv", "pnpm", "maturin", "ruff", "poetry", "pipx"].includes(a.split("=")[0])
            })
    ) {
        return true;
    }
    if (exe == "dotnet" && args.includes("tool") && args.includes("install")) {
        return true;
    }
    false
}

function rust_build(words: string[]) -> bool {
    let w = executable_words(words);
    let exe = w.at(0) ?? "";
    let args = w.slice(1, w.length());
    if (["cargo", "cross"].includes(exe)) {
        if (args.includes("miri")) {
            return false;
        }
        return args.some((a) -> {
            [
                "build",
                "b",
                "check",
                "c",
                "test",
                "t",
                "run",
                "r",
                "clippy",
                "doc",
                "bench",
                "nextest",
                "insta",
                "llvm-cov",
            ]
                .includes(a)
        });
    }
    exe == "wasm-pack"
        && args.some((a) -> {
            ["build", "test"].includes(a)
        })
}
