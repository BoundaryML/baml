// Durable memory: one Markdown file per fact, recalled by keyword overlap.

class Entry {
    name: string,
    keywords: string[],
    body: string,

    function score(self, tokens: string[]) -> int {
        let keys = _tokenize(`${self.name} ${self.keywords.join(" ")}`);
        let body_tokens = _tokenize(self.body);
        let total = 0;
        for (let token in tokens) {
            if (
                keys.some((candidate: string) -> bool {
                    _token_matches(candidate, token)
                })
            ) {
                total = total + 3;
            };
            if (
                body_tokens.some((candidate: string) -> bool {
                    _token_matches(candidate, token)
                })
            ) {
                total = total + 1;
            };
        }
        total
    }

    function marker(self) -> string {
        `[memory:${self.name}]`
    }

    function render(self) -> string {
        let hints = if (self.keywords.length() > 0) {
            ` (${self.keywords.join(", ")})`
        } else {
            ""
        };
        `${self.marker()}${hints} ${self.body}`
    }

    function to_file(self) -> string {
        `---\nname: ${self.name}\nkeywords: ${self.keywords.join(", ")}\n---\n${self.body}` + "\n"
    }
}

class Recall {
    block: string,
    names: string[],
}

class Hit {
    entry: Entry,
    score: int,
}

class Store {
    dir: string,

    function ensure(self) -> void {
        if (!baml.fs.exists(self.dir)) {
            baml.fs.mkdir(self.dir, baml.fs.MkdirOptions { recursive: true });
        };
    }

    function path_for(self, name: string) -> string {
        `${self.dir}/${_slugify(name)}.md`
    }

    function remember(self, name: string, keywords: string[], body: string) -> Entry {
        //# Persist one durable fact as readable Markdown
        self.ensure();
        let memory = Entry {
            name: _slugify(name),
            keywords: _normalize_keywords(keywords),
            body: body.trim(),
        };
        baml.fs.write(self.path_for(memory.name), memory.to_file());
        memory
    }

    function get(self, name: string) -> Entry? {
        let path = self.path_for(name);
        if (!baml.fs.exists(path)) {
            return null;
        };
        _parse_memory(_slugify(name), baml.fs.read(path))
    }

    function forget(self, name: string) -> bool {
        let path = self.path_for(name);
        if (!baml.fs.exists(path)) {
            return false;
        };
        baml.fs.remove(path);
        true
    }

    function all(self) -> Entry[] {
        if (!baml.fs.exists(self.dir)) {
            return [];
        };
        let memories: Entry[] = [];
        for (let entry in baml.fs.read_dir(self.dir)) {
            if (entry.is_file && entry.name.ends_with(".md")) {
                let slug = entry.name.substring(0, entry.name.length() - 3);
                //# One unreadable file must not take down the whole store
                let parsed = _parse_memory(slug, baml.fs.read(`${self.dir}/${entry.name}`)) catch_all (error) {
                    _ => {
                        continue;
                    },
                };
                memories.push(parsed);
            };
        }
        //# Filesystem order is not deterministic; name order is
        memories.sort_by_key((memory: Entry) -> string {
            memory.name
        });
        memories
    }

    function search(self, query: string, limit: int = 3) -> Entry[] {
        //# Score every memory once against the current user message.
        //# Deduplicated tokens: a repeated word must not double-count.
        let tokens = _dedup(_tokenize(query));
        if (tokens.length() == 0) {
            return [];
        };
        let hits = self.all()
            .map((memory: Entry) -> Hit {
                Hit { entry: memory, score: memory.score(tokens) }
            })
            .filter((hit: Hit) -> bool {
                hit.score > 0
            });
        hits.sort_by_key((hit: Hit) -> int {
            0 - hit.score
        });
        hits.slice(0, limit.min(hits.length())).map((hit: Hit) -> Entry {
            hit.entry
        })
    }

    function inject(self, query: string, history: string, limit: int = 3) -> Recall {
        //# Inject relevant facts only once per application transcript
        let fresh = self.search(query, limit = limit).filter((memory: Entry) -> bool {
            !history.includes(memory.marker())
        });
        if (fresh.length() == 0) {
            return Recall { block: "", names: [] };
        };
        //# Cap each injected body so recall cannot flood the prompt
        let lines = fresh.map((memory: Entry) -> string {
            _clip(memory.render(), 600)
        });
        Recall {
            block: "\n[RECALLED MEMORY — durable facts from earlier sessions; trust them unless the user contradicts them]\n"
                + lines.join("\n")
                + "\n",
            names: fresh.map((memory: Entry) -> string {
                memory.name
            }),
        }
    }

    function index(self) -> string {
        let memories = self.all();
        if (memories.length() == 0) {
            return "(memory is empty)";
        };
        //# An index, not a dump: bodies are clipped, never omitted — the
        //# curator needs them to spot obsolete or wrong facts
        memories
            .map((memory: Entry) -> string {
                `- ${memory.name} (${memory.keywords.join(", ")}): ${_clip(memory.body, 200)}`
            })
            .join("\n")
    }

    function listing(self) -> string {
        let memories = self.all();
        if (memories.length() == 0) {
            return `  (no memories yet — stored under ${self.dir}/)` + "\n";
        };
        let lines = memories.map((memory: Entry) -> string {
            `  • ${memory.name} (${memory.keywords.join(", ")})` + "\n    " + memory.body
        });
        let counted = if (memories.length() == 1) {
            "1 memory"
        } else {
            `${memories.length()} memories`
        };
        `  ${counted} in ${self.dir}/` + "\n" + lines.join("\n") + "\n"
    }
}

function default_store() -> Store {
    let configured = baml.env.get("BAML_MEMORY_AGENT_DIR") ?? baml.env.get("BAMLCODE_MEMORY_DIR")
        ?? ".bamlcode/memory";
    Store { dir: configured }
}

function enabled() -> bool {
    let configured = baml.env.get("BAML_MEMORY_AGENT") ?? baml.env.get("BAMLCODE_MEMORY") ?? "on";
    configured.trim().to_lower_case() != "off"
}
