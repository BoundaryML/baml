// 40 — application view of opinionated built-ins plus fresh/continue/resume/
// resume-at/fork session pickup. Session state is queryable without a model run.

class s40_Patch {
  files_changed: string[],
  summary: string,
}

function s40_new_store() -> s40_MemoryStore throws never {
  s40_MemoryStore { records: {}, next_id: 1, revision: 0 }
}

function s40_default_catalog() -> root.ai.Provider throws never {
  s40_ScriptedCatalog {
    store: s40_new_store(),
    cwd: "/workspace/project",
    cwd_keyed: true,
    reply_json: `{"files_changed":["auth.baml"],"summary":"fixed"}`,
  }
}

// The source design spells this as an explicit SessionCatalog companion because
// a subprocess harness does not pretend to implement Generate. This commented
// task sketch records the declaration whose request layer we want to inspect:
// function s40_Fix(task: string) -> s40_Patch {
//   client: s40_default_catalog()
//   prompt: `Fix ${task}. ${ctx.output_format}`
// }

// Hand-written reference expansion of the compiler task surface for `s40_Fix`.
// These helper names are scaffolding, not literal generated names:
// - `s40_Fix_request` represents `s40_Fix.request(...)`.
// - `s40_Fix_manual` is a custom SessionCatalog consumer that adds pickup
//   policy; it is not a compiler-generated companion or the plain `run` lowering.
function s40_Fix_request(
  task: string,
  provider: root.ai.Provider = s40_default_catalog(),
) -> root.ai.Request<s40_Patch> {
  root.ai.task_request<s40_Patch>(
    provider,
    "s40_Fix",
    "ns_ai_scenarios",
    { "task": task },
    [],
    root.ai.RequestOptions {},
    { "scenario": "40", "resource": "session-catalog" },
    s40_prompt_template(`Fix ${task}.`, true),
  )
}

function s40_Fix_manual(
  task: string,
  pickup: s40_Pickup = s40_Fresh {},
  provider: root.ai.Provider = s40_default_catalog(),
) -> s40_Patch throws s40_SessionError | baml.errors.UnknownError {
  let request = s40_Fix_request(task, provider = provider);
  match (provider) {
    let catalog: s40_SessionCatalog => catalog.run_in_session<s40_Patch>(request, pickup),
    _ => throw baml.errors.UnknownError {
      data: provider,
      message: ["provider has no queryable session catalog"],
    },
  }
}

function s40_TriageSessions(
  provider: root.ai.Provider,
  incident_tag: string,
) -> s40_SessionInfo[] throws s40_SessionError | baml.errors.UnknownError {
  match (provider) {
    let catalog: s40_SessionCatalog => {
      let sessions = catalog.list_sessions();
      for (let session in sessions) {
        if (session.message_count > 0) {
          let _ = catalog.tag_session(session.id, incident_tag);
          let _ = catalog.rename_session(session.id, "triage: " + (session.title ?? session.id));
        }
      }
      sessions
    },
    _ => throw baml.errors.UnknownError { data: provider, message: ["provider is not queryable"] },
  }
}

function s40_AuditSession(
  provider: root.ai.Provider,
  session_id: string,
) -> root.ai.ChatMessage[] throws s40_SessionError | baml.errors.UnknownError {
  match (provider) {
    let catalog: s40_SessionCatalog => catalog.session_messages(session_id),
    _ => throw baml.errors.UnknownError { data: provider, message: ["provider is not queryable"] },
  }
}

function s40_ToolPalette(allow: string[]?, deny: string[]?) -> root.ai.Tool[] throws never {
  s40_BuiltinTools.resolve(allow, deny)
}

test "40: the harness ships a fixed palette and the app only allows or denies" {
  let tools = s40_ToolPalette(["Read", "Edit", "Bash", "Grep", "Glob", "Write"], ["Write"]);
  assert.equal(tools.map((tool: root.ai.Tool) -> string { tool.name }), ["Read", "Edit", "Bash", "Grep", "Glob"])
}

test "40: Fresh then Continue append to the same cwd-scoped transcript" {
  let store = s40_new_store();
  let provider = s40_ScriptedCatalog {
    store: store,
    cwd: "/workspace/project",
    cwd_keyed: true,
    reply_json: `{"files_changed":["auth.baml"],"summary":"fixed"}`,
  };
  let _first = s40_Fix_manual("fix auth", provider = provider);
  let _second = s40_Fix_manual("add tests", pickup = s40_Continue {}, provider = provider);
  let sessions = provider.list_sessions();
  assert.equal(sessions.length(), 1)
  assert.equal(sessions.at(0)?.message_count, 4)
}

test "40: moving cwd makes Continue fresh and named Resume reports not found" {
  let store = s40_new_store();
  let original = s40_ScriptedCatalog {
    store: store, cwd: "/workspace/project", cwd_keyed: true,
    reply_json: `{"files_changed":[],"summary":"original"}`,
  };
  let moved = s40_ScriptedCatalog {
    store: store, cwd: "/workspace/moved", cwd_keyed: true,
    reply_json: `{"files_changed":[],"summary":"moved"}`,
  };
  let _ = s40_Fix_manual("first", provider = original);
  let original_id = original.list_sessions().at(0)?.id ?? "missing";
  let _ = s40_Fix_manual("continue after move", pickup = s40_Continue {}, provider = moved);
  assert.equal(moved.list_sessions().length(), 1)
  let status = s40_Fix_manual(
    "resume old id",
    pickup = s40_Resume { session_id: original_id },
    provider = moved,
  ) catch (e) {
    let session_error: s40_SessionError => if (session_error.is_not_found()) { "not-found" } else { "session-error" },
    _ => "wrong-error",
  };
  assert.equal(status, "not-found")
}

test "40: Fork creates a new id seeded from the parent prefix" {
  let store = s40_new_store();
  let provider = s40_ScriptedCatalog {
    store: store, cwd: "/workspace/project", cwd_keyed: true,
    reply_json: `{"files_changed":[],"summary":"turn"}`,
  };
  let _ = s40_Fix_manual("first", provider = provider);
  let parent_id = provider.list_sessions().at(0)?.id ?? "missing";
  let _ = s40_Fix_manual(
    "different approach",
    pickup = s40_Fork { session_id: parent_id, at_message_index: null },
    provider = provider,
  );
  let sessions = provider.list_sessions();
  assert.equal(sessions.length(), 2)
  let children = sessions.filter((info: s40_SessionInfo) -> bool { info.parent_id == parent_id });
  assert.equal(children.length(), 1)
  assert.equal(children.at(0)?.message_count, 4)
}

test "40: ResumeAt truncates in place before the next turn" {
  let store = s40_new_store();
  let provider = s40_ScriptedCatalog {
    store: store, cwd: "/workspace/project", cwd_keyed: true,
    reply_json: `{"files_changed":[],"summary":"turn"}`,
  };
  let _ = s40_Fix_manual("first", provider = provider);
  let _ = s40_Fix_manual("second", pickup = s40_Continue {}, provider = provider);
  let id = provider.list_sessions().at(0)?.id ?? "missing";
  let _ = s40_Fix_manual(
    "replace the second turn",
    pickup = s40_ResumeAt { session_id: id, message_index: 1 },
    provider = provider,
  );
  assert.equal(provider.list_sessions().length(), 1)
  assert.equal(provider.session_info(id).message_count, 4)
}

test "40: the non-cwd-keyed escape hatch resumes after a project move" {
  let store = s40_new_store();
  let before_move = s40_ScriptedCatalog {
    store: store, cwd: "/workspace/project", cwd_keyed: false,
    reply_json: `{"files_changed":[],"summary":"portable"}`,
  };
  let after_move = s40_ScriptedCatalog {
    store: store, cwd: "/workspace/moved", cwd_keyed: false,
    reply_json: `{"files_changed":[],"summary":"portable"}`,
  };
  let _ = s40_Fix_manual("first", provider = before_move);
  let id = before_move.list_sessions().at(0)?.id ?? "missing";
  let result = s40_Fix_manual(
    "resume after move",
    pickup = s40_Resume { session_id: id },
    provider = after_move,
  );
  assert.equal(result.summary, "portable")
  assert.equal(after_move.session_info(id).message_count, 4)
}

test "40: list, tag, rename, and audit do not run another agent turn" {
  let store = s40_new_store();
  let provider = s40_ScriptedCatalog {
    store: store, cwd: "/workspace/project", cwd_keyed: true,
    reply_json: `{"files_changed":[],"summary":"fixed"}`,
  };
  let _ = s40_Fix_manual("fix", provider = provider);
  let info = s40_TriageSessions(provider, "incident-42").at(0);
  assert.equal(info?.tags, ["incident-42"])
  assert.is_true((info?.title ?? "").starts_with("triage:"))
  assert.equal(s40_AuditSession(provider, info?.id ?? "missing").length(), 2)
}

test "40: catalog history is explicitly a portable Conversation view" {
  let store = s40_new_store();
  let provider = s40_ScriptedCatalog {
    store: store,
    cwd: "/workspace/project",
    cwd_keyed: true,
    reply_json: `{"files_changed":[],"summary":"portable"}`,
  };
  let _ = s40_Fix_manual("record a portable turn", provider = provider);
  let id = provider.list_sessions().at(0)?.id ?? "missing";
  let conversation = provider.session_conversation(id);
  assert.equal(conversation.messages.length(), 2)
  assert.equal(provider.session_token(id), null)
}

testset "integ-test-40-harness-sessions" {
  test "40 integ: the queryable transcript can be materialized as cwd-keyed JSONL" {
    let store = s40_new_store();
    let provider = s40_ScriptedCatalog {
      store: store, cwd: "/workspace/project", cwd_keyed: true,
      reply_json: `{"files_changed":[],"summary":"persisted"}`,
    };
    let _ = s40_Fix_manual("persist this turn", provider = provider);
    let id = provider.list_sessions().at(0)?.id ?? "missing";
    let record = store.require(id, "/workspace/project", true);
    let layout = s40_JsonlLayout {
      root: (baml.env.get("TMPDIR") ?? "/tmp") + "/baml-s40-sessions",
      cwd: "/workspace/project",
    };
    defer { baml.fs.remove_dir_all(layout.root) catch_all (e) { _ => null } }
    let bytes = s40_write_jsonl_snapshot(layout, record);
    assert.is_true(bytes > 0)
    assert.equal(baml.fs.read(layout.file(id)).lines().length(), 2)
  }

  test "40 live: one OpenAI-backed catalog continues the same local session" {
    let store = s40_new_store();
    let provider = s40_LiveCatalog {
      store: store,
      cwd: "/workspace/project",
      inner: root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    };
    let first = s40_Fix_manual("Return a one-file patch plan for auth.", provider = provider);
    let id = provider.list_sessions().at(0)?.id ?? "missing";
    let second = s40_Fix_manual(
      "Refine that plan with a regression test.",
      pickup = s40_Resume { session_id: id },
      provider = provider,
    );
    assert.is_true(first.summary.length() > 0)
    assert.is_true(second.summary.length() > 0)
    assert.equal(provider.session_info(id).message_count, 4)
  }
}
