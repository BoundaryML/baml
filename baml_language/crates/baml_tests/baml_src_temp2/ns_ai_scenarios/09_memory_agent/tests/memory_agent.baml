// Deterministic coverage for the memory-agent scenario.
//
// These tests exercise the application tools, terminal state machine, durable
// memory store, recall injection, curator edit application, and provider
// selection without making any model requests.

testset "memory-agent-tools" {
    test "write then read round trips with line numbers" {
        let path = "/tmp/baml_memory_agent_test/tools/read.txt";
        let wrote = _memory_agent_write_file(path, "alpha\nbeta\n");
        assert.contains(wrote, "Wrote");

        let read = _memory_agent_read_file(path);
        assert.contains(read, "1\talpha");
        assert.contains(read, "2\tbeta")
    }

    test "edit replaces only the first exact occurrence" {
        let path = "/tmp/baml_memory_agent_test/tools/edit.txt";
        _memory_agent_write_file(path, "old and old");
        let edited = _memory_agent_edit_file(path, "old", "new");
        assert.contains(edited, "Edited");
        assert.contains(_memory_agent_read_file(path), "new and old")
    }

    test "edit miss is a recoverable error" {
        let path = "/tmp/baml_memory_agent_test/tools/edit-miss.txt";
        _memory_agent_write_file(path, "hello");
        assert.contains(_memory_agent_edit_file(path, "missing", "replacement"), "ERROR")
    }

    test "reading a missing file is a recoverable error" {
        assert
            .contains(
                _memory_agent_read_file("/tmp/baml_memory_agent_test/tools/definitely-does-not-exist.txt"),
                "ERROR",
            )
    }

    test "list dir identifies files and directories" {
        let dir = "/tmp/baml_memory_agent_test/tools/list";
        _memory_agent_write_file(`${dir}/file.txt`, "file");
        _memory_agent_write_file(`${dir}/nested/child.txt`, "child");
        let listing = _memory_agent_list_dir(dir);
        assert.contains(listing, "file.txt");
        assert.contains(listing, "nested/")
    }

    test "bash reports stdout and successful exit code" {
        let result = _memory_agent_run_bash("echo hello-memory-agent");
        assert.contains(result, "exit code: 0");
        assert.contains(result, "hello-memory-agent")
    }

    test "bash reports nonzero exit code without throwing" {
        assert.contains(_memory_agent_run_bash("exit 3"), "exit code: 3")
    }

    test "main tool roster contains filesystem tools but no memory mutations" {
        let tools = root.ai.tools.normalize_tools(_memory_agent_main_tools());
        let names = tools.map((tool: root.ai.tools.Tool) -> string {
            tool.name
        });
        assert.equal(names, ["read_file", "write_file", "edit_file", "list_dir", "run_bash"]);
        assert.is_true(!names.includes("remember"));
        assert.is_true(!names.includes("forget"))
    }
}

testset "memory-agent-queue" {
    test "queue command strips either accepted prefix" {
        assert.equal(_memory_agent_command_arg("/queue fix the bug"), "fix the bug");
        assert.equal(_memory_agent_command_arg("/q add tests"), "add tests")
    }

    test "queue display preserves order and numbering" {
        let display = _memory_agent_show_queue(["first", "second"]);
        assert.contains(display, "1.");
        assert.contains(display, "first");
        assert.contains(display, "2.");
        assert.contains(display, "second")
    }

    test "empty queue has an explicit empty state" {
        let queue: string[] = [];
        assert.contains(_memory_agent_show_queue(queue), "empty")
    }

    test "prompt label reports staged message count" {
        assert.contains(_memory_agent_prompt_label(["a", "b", "c"]), "3 queued");
        let queue: string[] = [];
        assert.contains(_memory_agent_prompt_label(queue), "you")
    }
}

testset "memory-agent-interrupt" {
    test "escape followed by enter cancels the active turn" {
        let interrupt = _memory_agent_classify_interrupt(string.from_code_points([27]));
        assert.is_true(interrupt.cancel);
        assert.equal(interrupt.queued, "")
    }

    test "empty input is ignored while a turn runs" {
        let interrupt = _memory_agent_classify_interrupt("   ");
        assert.is_true(!interrupt.cancel);
        assert.equal(interrupt.queued, "")
    }

    test "other input becomes typeahead instead of cancellation" {
        let interrupt = _memory_agent_classify_interrupt("also write docs");
        assert.is_true(!interrupt.cancel);
        assert.equal(interrupt.queued, "also write docs");
        assert.equal(_memory_agent_classify_interrupt("cancel").queued, "cancel")
    }
}

testset "memory-agent-transcript" {
    test "recalled memory appears before the new user message" {
        let store = _memory_agent_fresh_store("agent-wiring");
        store.remember("prefers-uv", ["python", "deps"], "Install deps with `uv add`.");

        let recall = store.inject("add a python dep", "");
        let transcript = _memory_agent_opening_transcript("EARLIER\n", recall.block, "add a python dep");

        assert.contains(transcript, "uv add");
        assert
            .is_true(
                (transcript.index_of("RECALLED MEMORY") ?? -1)
                    < (transcript.index_of("USER: add a python dep") ?? -1),
            );
        assert.is_true(transcript.starts_with("EARLIER\n"))
    }

    test "no recall leaves ordinary transcript shape unchanged" {
        assert
            .equal(
                _memory_agent_opening_transcript("EARLIER\n", "", "hello"),
                "EARLIER\n\nUSER: hello\n",
            )
    }

    test "curator receives only the newly completed turn" {
        assert
            .equal(
                _memory_agent_turn_delta("OLD\n", "OLD\nUSER: hi\nASSISTANT: yo\n"),
                "USER: hi\nASSISTANT: yo\n",
            );
        assert.equal(_memory_agent_turn_delta("OLD\n", "DIFFERENT\n"), "DIFFERENT\n")
    }
}

testset "memory-agent-memory-text" {
    test "slugify creates stable file-safe identifiers" {
        assert.equal(_memory_agent_slugify("Prefers UV"), "prefers-uv");
        assert.equal(_memory_agent_slugify("  deploy/  staging!! "), "deploy-staging");
        assert.equal(_memory_agent_slugify("!!!"), "memory")
    }

    test "tokenize keeps meaningful terms and drops noise" {
        assert.equal(_memory_agent_tokenize("How do I run the tests?"), ["run", "tests"]);
        assert.equal(_memory_agent_tokenize("uv add pydantic"), ["uv", "add", "pydantic"]);
        assert.equal(_memory_agent_tokenize("   "), [])
    }

    test "token matching tolerates plurals but not accidental prefixes" {
        assert.is_true(_memory_agent_token_matches("test", "tests"));
        assert.is_true(_memory_agent_token_matches("deploy", "deploy"));
        assert.is_true(!_memory_agent_token_matches("dep", "deploy"));
        assert.is_true(!_memory_agent_token_matches("python", "pytest"))
    }

    test "keywords are normalized and blanks are removed" {
        assert
            .equal(
                _memory_agent_split_keywords(" Python , DEPS,, install "),
                ["python", "deps", "install"],
            )
    }

    test "memory file format is human readable and round trips through the store" {
        let store = _memory_agent_fresh_store("file-format");
        store.remember("Prefers UV", ["Python", "deps"], "Use `uv add`.");
        let raw = baml.fs.read(store.path_for("Prefers UV"));
        assert.contains(raw, "name: prefers-uv");
        assert.contains(raw, "keywords: python, deps");
        assert.is_true(raw.ends_with("\n"));

        let memory = store.get("prefers-uv");
        assert.equal(memory?.name, "prefers-uv");
        assert.equal(memory?.keywords, ["python", "deps"]);
        assert.equal(memory?.body, "Use `uv add`.")
    }

    test "headerless hand-written memory uses its filename as the id" {
        let store = _memory_agent_fresh_store("headerless");
        baml.fs.write(`${store.dir}/scratch.md`, "just a note\nsecond line\n");
        let memory = store.get("scratch");
        assert.equal(memory?.name, "scratch");
        assert.equal(memory?.keywords, []);
        assert.equal(memory?.body, "just a note\nsecond line")
    }

    test "multiline memory bodies survive persistence" {
        let store = _memory_agent_fresh_store("multiline");
        store.remember("build", ["ci"], "line one\n\nline three");
        assert.equal(store.get("build")?.body, "line one\n\nline three")
    }
}

testset "memory-agent-memory-store" {
    test "remember and get round trip through a markdown file" {
        let store = _memory_agent_fresh_store("round-trip");
        store
            .remember("Prefers UV", ["Python", "deps"], "  The user installs deps with `uv add`.  ");

        let memory = store.get("prefers-uv");
        assert.not_null(memory);
        assert.equal(memory?.name, "prefers-uv");
        assert.equal(memory?.keywords, ["python", "deps"]);
        assert.equal(memory?.body, "The user installs deps with `uv add`.")
    }

    test "lookup is insensitive to slug spelling and case" {
        let store = _memory_agent_fresh_store("slug-lookup");
        store.remember("Deploy Steps", [], "run ./deploy.sh");
        assert.not_null(store.get("deploy steps"));
        assert.not_null(store.get("DEPLOY-STEPS"))
    }

    test "remember overwrites the same logical name" {
        let store = _memory_agent_fresh_store("overwrite");
        store.remember("port", ["server"], "the dev server runs on 3000");
        store.remember("port", ["server"], "the dev server runs on 8080");
        assert.equal(store.all().length(), 1);
        assert.equal(store.get("port")?.body, "the dev server runs on 8080")
    }

    test "forget removes a memory and is idempotent" {
        let store = _memory_agent_fresh_store("forget");
        store.remember("temp", [], "throwaway");
        assert.is_true(store.forget("temp"));
        assert.equal(store.get("temp"), null);
        assert.is_true(!store.forget("temp"))
    }

    test "hand-written markdown is discovered but other files are ignored" {
        let store = _memory_agent_fresh_store("discovery");
        baml.fs.write(`${store.dir}/dropped.md`, "a hand-written fact");
        baml.fs.write(`${store.dir}/notes.txt`, "not a memory");
        assert.equal(store.all().length(), 1);
        assert.equal(store.all().at(0)?.name, "dropped")
    }

    test "empty store is usable" {
        let store = _memory_agent_fresh_store("empty");
        assert.equal(store.all(), []);
        assert.equal(store.search("anything at all"), []);
        assert.contains(store.listing(), "no memories yet")
    }
}

testset "memory-agent-memory-recall" {
    test "search ranks relevant memories and excludes unrelated ones" {
        let store = _memory_agent_fresh_store("recall-ranks");
        store
            .remember(
                "prefers-uv",
                ["python", "deps"],
                "Install python deps with `uv add`, never pip.",
            );
        store
            .remember("test-command", ["tests", "pytest"], "Run the suite with `uv run pytest -q`.");
        store
            .remember(
                "deploy-steps",
                ["deploy", "prod"],
                "Deploys go out via `make ship` on Tuesdays.",
            );

        let hits = store.search("how do I add a python dependency?");
        assert.equal(hits.length(), 1);
        assert.equal(hits.at(0)?.name, "prefers-uv");
        assert.equal(store.search("what colour is the sky"), [])
    }

    test "keyword hits outrank body-only hits" {
        let store = _memory_agent_fresh_store("recall-ranking");
        store.remember("keyworded", ["deploy"], "unrelated body text");
        store.remember("body-only", ["misc"], "this body mentions deploy once");
        let hits = store.search("deploy");
        assert.equal(hits.length(), 2);
        assert.equal(hits.at(0)?.name, "keyworded")
    }

    test "search limit and stopword filtering prevent prompt flooding" {
        let store = _memory_agent_fresh_store("recall-limit");
        store.remember("a", ["deploy"], "one");
        store.remember("b", ["deploy"], "two");
        store.remember("c", ["deploy"], "three");
        assert.equal(store.search("deploy").length(), 3);
        assert.equal(store.search("deploy", limit = 2).length(), 2);
        assert.equal(store.search("what is that"), [])
    }

    test "inject includes only relevant unused memories" {
        let store = _memory_agent_fresh_store("inject-relevant");
        store
            .remember(
                "prefers-uv",
                ["python", "deps"],
                "Install python deps with `uv add`, never pip.",
            );
        store.remember("deploy-steps", ["deploy", "prod"], "Deploy via `make ship`.");

        let recall = store.inject("add a python dep", "");
        assert.equal(recall.names, ["prefers-uv"]);
        assert.contains(recall.block, "RECALLED MEMORY");
        assert.contains(recall.block, "[memory:prefers-uv]");
        assert.contains(recall.block, "uv add");
        assert.is_true(!recall.block.includes("make ship"))
    }

    test "inject is idempotent but permits a different later memory" {
        let store = _memory_agent_fresh_store("inject-idempotent");
        store.remember("prefers-uv", ["python", "deps"], "Use `uv add`.");
        store.remember("deploy-steps", ["deploy", "prod"], "Deploy via `make ship`.");

        let first = store.inject("python deps", "");
        let history = `USER: python deps${first.block}`;
        let duplicate = store.inject("another python dep question", history);
        assert.equal(duplicate.block, "");
        assert.equal(duplicate.names, []);

        let fresh = store.inject("time to deploy to prod", history);
        assert.equal(fresh.names, ["deploy-steps"])
    }

    test "listing summarizes the durable store" {
        let store = _memory_agent_fresh_store("listing");
        store.remember("prefers-uv", ["python"], "Use uv.");
        store.remember("deploy-steps", ["deploy"], "Use make ship.");
        let listing = store.listing();
        assert.contains(listing, "prefers-uv");
        assert.contains(listing, "deploy-steps");
        assert.contains(listing, "2 memories")
    }
}

testset "memory-agent-curator" {
    test "save and delete edits mutate the store" {
        let store = _memory_agent_fresh_store("curator-apply");
        store.remember("stale", ["old"], "the port is 3000");
        let applied = _memory_agent_apply_edits(
            [
                _memory_agent_save_edit("test-command", ["tests"], "run `uv run pytest -q`"),
                _memory_agent_delete_edit("stale"),
            ],
            store,
        );
        assert.equal(applied, ["+test-command", "-stale"]);
        assert.equal(store.get("test-command")?.body, "run `uv run pytest -q`");
        assert.equal(store.get("stale"), null)
    }

    test "invalid edits are discarded" {
        let store = _memory_agent_fresh_store("curator-invalid");
        let applied = _memory_agent_apply_edits(
            [
                _memory_agent_save_edit("", ["x"], "no name"),
                _memory_agent_save_edit("no-body", ["x"], "   "),
                _memory_agent_delete_edit("never-existed"),
            ],
            store,
        );
        assert.equal(applied, []);
        assert.equal(store.all(), [])
    }

    test "save corrects an existing fact in place" {
        let store = _memory_agent_fresh_store("curator-correct");
        store.remember("port", ["server"], "the dev server runs on 3000");
        _memory_agent_apply_edits([_memory_agent_save_edit("port", ["server"], "the dev server runs on 8080")], store);
        assert.equal(store.all().length(), 1);
        assert.equal(store.get("port")?.body, "the dev server runs on 8080")
    }

    test "edit application is capped at five mutations per turn" {
        let store = _memory_agent_fresh_store("curator-cap");
        let edits = [
            _memory_agent_save_edit("m1", [], "one"),
            _memory_agent_save_edit("m2", [], "two"),
            _memory_agent_save_edit("m3", [], "three"),
            _memory_agent_save_edit("m4", [], "four"),
            _memory_agent_save_edit("m5", [], "five"),
            _memory_agent_save_edit("m6", [], "six"),
            _memory_agent_save_edit("m7", [], "seven"),
        ];
        assert.equal(_memory_agent_apply_edits(edits, store).length(), 5);
        assert.equal(store.all().length(), 5)
    }

    test "memory index is compact and includes existing facts" {
        let store = _memory_agent_fresh_store("curator-index");
        assert.equal(store.index(), "(memory is empty)");
        store.remember("prefers-uv", ["python"], "uses uv");
        assert.equal(store.index(), "- prefers-uv (python): uses uv")
    }
}

testset "memory-agent-providers" {
    test "task defaults to OpenAI and carries the main tool roster" {
        let task = _memory_agent_task("", "hello");
        assert.equal(task.provider_name(), "openai");
        assert.contains(task.prompt.text(), "USER: hello");
        assert
            .equal(
                task.tools.map((tool: root.ai.tools.Tool) -> string {
                    tool.name
                }),
                ["read_file", "write_file", "edit_file", "list_dir", "run_bash"],
            )
    }

    test "Google entrypoint overrides the same task contract" {
        let openai = _memory_agent_task("EARLIER\n", "hello");
        let google = _memory_agent_google_task("EARLIER\n", "hello");
        assert.equal(openai.provider_name(), "openai");
        assert.equal(google.provider_name(), "google-ai");
        assert.equal(openai.identity?.name, google.identity?.name);
        assert
            .equal(
                openai.tools.map((tool: root.ai.tools.Tool) -> string {
                    tool.name
                }),
                google.tools.map((tool: root.ai.tools.Tool) -> string {
                    tool.name
                }),
            )
    }

    test "provider settings request serial tools and preserve output budgets" {
        let openai = _memory_agent_openai_provider();
        let google = _memory_agent_google_provider();
        assert.is_true(!openai.parallel_tool_calls);
        assert.is_true(!google.parallel_tool_calls);
        assert
            .equal(
                baml.json.path<int>(baml.json.to_json(openai.extra_body), ".max_output_tokens"),
                4096,
            );
        assert
            .equal(
                baml.json
                    .path<int>(
                        baml.json.to_json(google.extra_body),
                        ".generationConfig.maxOutputTokens",
                    ),
                4096,
            );
        assert
            .equal(
                baml.json
                    .path<int>(
                        baml.json.to_json(_memory_agent_curator_provider().extra_body),
                        ".max_output_tokens",
                    ),
                1024,
            );
        assert
            .equal(
                baml.json
                    .path<int>(
                        baml.json.to_json(_memory_agent_google_curator_provider().extra_body),
                        ".generationConfig.maxOutputTokens",
                    ),
                1024,
            );
        assert.equal(_memory_agent_main_budget().max_steps, 30);
        assert.equal(_memory_agent_curator_budget().max_steps, 1)
    }

    test "Google override also applies to the isolated curator task" {
        let openai = _memory_agent_curator_task("USER: hi", "(memory is empty)");
        let google = _memory_agent_google_curator_task("USER: hi", "(memory is empty)");
        assert.equal(openai.provider_name(), "openai");
        assert.equal(google.provider_name(), "google-ai");
        assert.equal(openai.identity?.name, google.identity?.name);
        assert.equal(openai.tools, []);
        assert.equal(google.tools, [])
    }
}

testset "memory-agent-framework" {
    test "Task run Agent executes and correlates one coding tool" {
        let provider = root.ai.testing.ScriptedToolProvider {
            turns: [
                root.ai.tools.ToolCalls {
                    calls: [
                        root.ai.tools.ToolCall {
                            id: "memory-call-1",
                            name: "list_dir",
                            args: { "path": "/tmp", "summary": "Inspect the temporary directory." },
                        },
                    ],
                },
            ],
            final_output: "inspection complete",
            usage_per_step: null,
        };
        let trace = MemoryAgentTrace { transcript: "USER: inspect /tmp\n", steps: 0 };
        let observer = MemoryAgentObserver { trace: trace, live: false };
        let outcome = _memory_agent_task("", "inspect /tmp")
            .with_provider(provider)
            .run(
                runner = root
                    .ai
                    .run
                    .Agent<string>
                    .new(
                        budget = root.ai.Budget { max_steps: 3, max_cost_usd: null },
                        observers = [observer],
                    ),
            );
        match (outcome) {
            let done: root.ai.Done<string> => {
                assert.equal(done.value, "inspection complete");
                match (done.conversation) {
                    let state: root.ai.testing.ScriptedToolConversation => {
                        assert.equal(state.submitted_results.length(), 1);
                        assert.equal(state.submitted_results.at(0)?.at(0)?.id, "memory-call-1");
                        assert.is_true(!(state.submitted_results.at(0)?.at(0)?.is_error ?? true))
                    },
                    _ => assert.is_true(false),
                }
            },
            _ => assert.is_true(false),
        }
        assert.equal(trace.steps, 2);
        assert.contains(trace.transcript, "[THOUGHT] Inspect the temporary directory.");
        assert.contains(trace.transcript, "[ACTION] ls /tmp");
        assert.contains(trace.transcript, "[RESULT]")
    }

    test "Agent budget stops before an additional provider step" {
        let provider = root.ai.testing.ScriptedToolProvider {
            turns: [
                root.ai.tools.ToolCalls {
                    calls: [
                        root.ai.tools.ToolCall {
                            id: "budget-call-1",
                            name: "list_dir",
                            args: { "path": "/tmp", "summary": "Inspect once." },
                        },
                    ],
                },
                root.ai.tools.ToolCalls {
                    calls: [
                        root.ai.tools.ToolCall {
                            id: "budget-call-2",
                            name: "list_dir",
                            args: { "path": ".", "summary": "Inspect twice." },
                        },
                    ],
                },
            ],
            final_output: "never reached",
            usage_per_step: null,
        };
        let outcome = _memory_agent_task("", "inspect")
            .with_provider(provider)
            .run(
                runner = root
                    .ai
                    .run
                    .Agent<string>
                    .new(budget = root.ai.Budget { max_steps: 1, max_cost_usd: null }),
            );
        match (outcome) {
            let budget: root.ai.BudgetReached => {
                assert.equal(budget.steps_taken, 1);
                assert.equal(budget.reason, "max_steps")
            },
            _ => assert.is_true(false),
        }
    }

    test "empty curator turn is skipped without a model call" {
        assert
            .equal(
                _memory_agent_curate_turn("   ", _memory_agent_fresh_store("empty-curator"), MemoryAgentBackend.OpenAI),
                [],
            )
    }
}
