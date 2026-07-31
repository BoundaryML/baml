// Pure text utilities: tokenizing, slugifying, memory-file parsing, and
// transcript assembly.

function _tokenize(text: string) -> string[] {
    let stops = _stopwords();
    let tokens: string[] = [];
    let current = "";
    for (let character in text.to_lower_case().chars()) {
        if (character.is_ascii_alphanumeric()) {
            current = current + character;
        } else {
            if (_keep_token(current, stops)) {
                tokens.push(current);
            };
            current = "";
        };
    }
    if (_keep_token(current, stops)) {
        tokens.push(current);
    };
    tokens
}

function _keep_token(token: string, stops: string[]) -> bool {
    token.length() >= 2 && !stops.includes(token)
}

function _stopwords() -> string[] {
    [
        "am",
        "an",
        "as",
        "at",
        "be",
        "by",
        "do",
        "if",
        "in",
        "is",
        "it",
        "me",
        "my",
        "no",
        "of",
        "on",
        "or",
        "so",
        "to",
        "up",
        "us",
        "we",
        "the",
        "and",
        "for",
        "you",
        "your",
        "our",
        "with",
        "that",
        "this",
        "from",
        "have",
        "has",
        "are",
        "was",
        "were",
        "can",
        "will",
        "how",
        "what",
        "when",
        "why",
        "who",
        "all",
        "any",
        "but",
        "not",
        "get",
        "got",
        "let",
        "use",
        "into",
        "onto",
        "its",
        "his",
        "her",
        "them",
        "they",
        "than",
        "then",
        "there",
        "here",
        "some",
        "just",
        "like",
        "make",
        "made",
        "does",
        "did",
    ]
}

function _dedup(tokens: string[]) -> string[] {
    let seen: string[] = [];
    for (let token in tokens) {
        if (!seen.includes(token)) {
            seen.push(token);
        };
    }
    seen
}

function _token_matches(left: string, right: string) -> bool {
    if (left == right) {
        return true;
    };
    let shorter = if (left.length() < right.length()) {
        left
    } else {
        right
    };
    let longer = if (left.length() < right.length()) {
        right
    } else {
        left
    };
    shorter.length() >= 4 && longer.starts_with(shorter)
}

function _slugify(name: string) -> string {
    let output = "";
    for (let character in name.to_lower_case().chars()) {
        if (character.is_ascii_alphanumeric()) {
            output = output + character;
        } else if (output.length() > 0 && !output.ends_with("-")) {
            output = output + "-";
        };
    }
    let trimmed = if (output.ends_with("-")) {
        output.substring(0, output.length() - 1)
    } else {
        output
    };
    if (trimmed.length() == 0) {
        "memory"
    } else {
        trimmed
    }
}

function _parse_memory(slug: string, raw: string) -> Entry {
    let lines = raw.lines();
    if ((lines.at(0) ?? "").trim() != "---") {
        return Entry { name: slug, keywords: [], body: raw.trim() };
    };
    let name = slug;
    let keywords: string[] = [];
    let body: string[] = [];
    let in_header = true;
    let index = 1;
    while (index < lines.length()) {
        let line = lines[index];
        index = index + 1;
        if (in_header) {
            if (line.trim() == "---") {
                in_header = false;
            } else if (line.starts_with("name:")) {
                let value = _header_value(line, "name:");
                if (value.length() > 0) {
                    name = value;
                };
            } else if (line.starts_with("keywords:")) {
                keywords = _split_keywords(_header_value(line, "keywords:"));
            };
        } else {
            body.push(line);
        };
    }
    Entry { name: name, keywords: keywords, body: body.join("\n").trim() }
}

function _header_value(line: string, key: string) -> string {
    line.substring(key.length(), line.length()).trim()
}

function _split_keywords(raw: string) -> string[] {
    _normalize_keywords(raw.split(","))
}

function _normalize_keywords(raw: string[]) -> string[] {
    raw.map((keyword: string) -> string {
            keyword.trim().to_lower_case()
        })
        .filter((keyword: string) -> bool {
            keyword.length() > 0
        })
}

function _with_line_numbers(body: string) -> string {
    let output: string[] = [];
    let line_number = 0;
    for (let line in body.lines()) {
        line_number = line_number + 1;
        output.push(`${line_number}\t${line}`);
    }
    output.join("\n")
}

function _clip(text: string, limit: int) -> string {
    if (text.length() > limit) {
        text.substring(0, limit) + `\n… (${text.length() - limit} more chars truncated)`
    } else {
        text
    }
}

function _opening_transcript(history: string, recall_block: string, user_msg: string) -> string {
    history + recall_block + "\nUSER: " + user_msg + "\n"
}

function _turn_delta(history: string, transcript: string) -> string {
    if (transcript.starts_with(history)) {
        transcript.substring(history.length(), transcript.length())
    } else {
        transcript
    }
}

function _continuation_text(recall_block: string, user_msg: string) -> string {
    if (recall_block.trim().length() == 0) {
        user_msg
    } else {
        recall_block.trim() + "\n\n" + user_msg
    }
}
