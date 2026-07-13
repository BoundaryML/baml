// 40 — built-in harness tools plus queryable, resumable session storage.
// The real filesystem/subprocess boundary is represented by an in-memory
// catalog offline; the capability and pickup semantics match the design.

class s40_ReadArgs { path: string, offset: int?, limit: int? }
class s40_EditArgs { path: string, old: string, new: string, replace_all: bool? }
class s40_WriteArgs { path: string, content: string }
class s40_BashArgs { command: string, timeout_ms: int? }
class s40_GrepArgs { pattern: string, path: string?, glob: string? }
class s40_GlobArgs { pattern: string, path: string? }
class s40_LsArgs { path: string }

class s40_BuiltinTools {
  function all() -> root.ai.Tool[] throws never {
    [
      root.ai.Tool.typed<s40_ReadArgs>("Read", "Read a file from the workspace."),
      root.ai.Tool.typed<s40_EditArgs>("Edit", "Replace an exact string in a file."),
      root.ai.Tool.typed<s40_WriteArgs>("Write", "Write a file."),
      root.ai.Tool.typed<s40_BashArgs>("Bash", "Run a shell command in the sandbox."),
      root.ai.Tool.typed<s40_GrepArgs>("Grep", "Search file contents."),
      root.ai.Tool.typed<s40_GlobArgs>("Glob", "Find files by glob."),
      root.ai.Tool.typed<s40_LsArgs>("Ls", "List a directory."),
    ]
  }

  function resolve(allow: string[]?, deny: string[]?) -> root.ai.Tool[] throws never {
    s40_BuiltinTools.all().filter((tool: root.ai.Tool) -> bool {
      let allowed = match (allow) {
        let names: string[] => names.includes(tool.name),
        null => true,
      };
      let denied = match (deny) {
        let names: string[] => names.includes(tool.name),
        null => false,
      };
      allowed && !denied
    })
  }
}

class s40_Fresh {}
class s40_Continue {}
class s40_Resume { session_id: string }
class s40_ResumeAt { session_id: string, message_index: int }
class s40_Fork { session_id: string, at_message_index: int? }
type s40_Pickup = s40_Fresh | s40_Continue | s40_Resume | s40_ResumeAt | s40_Fork

interface s40_SessionError {
  function is_network_error(self) -> bool throws never
  function is_not_found(self) -> bool throws never
}

class s40_SessionNotFound {
  session_id: string?,
  message: string,

  implements s40_SessionError {
    function is_network_error(self) -> bool { false }
    function is_not_found(self) -> bool { true }
  }
}

implements baml.errors.Failure for s40_SessionNotFound {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { false }
  function is_policy_refusal(self) -> bool { false }
  function is_resumable(self) -> bool { true }
  function is_unsupported(self) -> bool { false }
}

class s40_SessionInfo {
  id: string,
  cwd: string,
  title: string?,
  tags: string[],
  message_count: int,
  revision: int,
  parent_id: string?,
  branch_point: int?,
}

class s40_SessionRecord {
  info: s40_SessionInfo,
  // Portable application history. Never substitute this for exact provider
  // continuation state when a HarnessSessionToken is available.
  turns: root.ai.ChatMessage[],
  continuation: root.ai.HarnessSessionToken?,
}

// The cwd-keyed JSONL layout is explicit even though unit tests use the memory
// store. Moving a project changes this directory and therefore orphans Continue.
class s40_JsonlLayout {
  root: string,
  cwd: string,

  function encoded_cwd(self) -> string throws never {
    self.cwd.chars().map((char: string) -> string {
      if (char.is_alphanumeric()) { char } else { "-" }
    }).join("")
  }

  function dir(self) -> string throws never { self.root + "/" + self.encoded_cwd() }
  function file(self, session_id: string) -> string throws never {
    self.dir() + "/" + session_id + ".jsonl"
  }
}

function s40_write_jsonl_snapshot(
  layout: s40_JsonlLayout,
  record: s40_SessionRecord,
) -> int {
  baml.fs.mkdir(layout.dir(), baml.fs.MkdirOptions { recursive: true });
  let lines = record.turns.map((turn: root.ai.ChatMessage) -> string {
    baml.json.to_string(turn)
  });
  baml.fs.write(layout.file(record.info.id), lines.join("\n") + "\n")
}

class s40_MemoryStore {
  records: map<string, s40_SessionRecord>,
  next_id: int,
  revision: int,

  function create(
    self,
    cwd: string,
    parent_id: string?,
    branch_point: int?,
    turns: root.ai.ChatMessage[],
  ) -> string throws never {
    let id = `session-${self.next_id}`;
    self.next_id = self.next_id + 1;
    self.revision = self.revision + 1;
    let _ = self.records.set(id, s40_SessionRecord {
      info: s40_SessionInfo {
        id: id, cwd: cwd, title: null, tags: [], message_count: turns.length(),
        revision: self.revision, parent_id: parent_id, branch_point: branch_point,
      },
      turns: turns,
      continuation: null,
    });
    id
  }

  function visible(self, cwd: string, cwd_keyed: bool) -> s40_SessionInfo[] throws never {
    self.records.values().filter((record: s40_SessionRecord) -> bool {
      !cwd_keyed || record.info.cwd == cwd
    }).map((record: s40_SessionRecord) -> s40_SessionInfo { record.info })
  }

  function require(
    self,
    id: string,
    cwd: string,
    cwd_keyed: bool,
  ) -> s40_SessionRecord throws s40_SessionNotFound {
    match (self.records.get(id)) {
      let record: s40_SessionRecord if !cwd_keyed || record.info.cwd == cwd => record,
      _ => throw s40_SessionNotFound {
        session_id: id,
        message: "session is missing or belongs to a different cwd",
      },
    }
  }

  function append(
    self,
    id: string,
    cwd: string,
    cwd_keyed: bool,
    turns: root.ai.ChatMessage[],
  ) -> null throws s40_SessionNotFound {
    let record = self.require(id, cwd, cwd_keyed);
    record.turns = record.turns.concat(turns);
    self.revision = self.revision + 1;
    record.info.message_count = record.turns.length();
    record.info.revision = self.revision;
    null
  }
}

function s40_most_recent(infos: s40_SessionInfo[]) -> s40_SessionInfo throws never {
  let best = infos.at(0) ?? s40_SessionInfo {
    id: "", cwd: "", title: null, tags: [], message_count: 0,
    revision: -1, parent_id: null, branch_point: null,
  };
  for (let info in infos) {
    if (info.revision > best.revision) { best = info; }
  }
  best
}

function s40_resolve_pickup(
  store: s40_MemoryStore,
  cwd: string,
  cwd_keyed: bool,
  pickup: s40_Pickup,
) -> string throws s40_SessionNotFound {
  match (pickup) {
    let fresh: s40_Fresh => store.create(cwd, null, null, []),
    let continuing: s40_Continue => {
      let visible = store.visible(cwd, cwd_keyed);
      if (visible.length() == 0) { store.create(cwd, null, null, []) }
      else { s40_most_recent(visible).id }
    },
    let resume: s40_Resume => store.require(resume.session_id, cwd, cwd_keyed).info.id,
    let at: s40_ResumeAt => {
      let record = store.require(at.session_id, cwd, cwd_keyed);
      let end = if (at.message_index < record.turns.length()) { at.message_index + 1 } else { record.turns.length() };
      record.turns = record.turns.slice(0, end);
      record.info.message_count = record.turns.length();
      record.info.id
    },
    let fork: s40_Fork => {
      let parent = store.require(fork.session_id, cwd, cwd_keyed);
      let end = match (fork.at_message_index) {
        let index: int => if (index < parent.turns.length()) { index + 1 } else { parent.turns.length() },
        null => parent.turns.length(),
      };
      store.create(cwd, parent.info.id, fork.at_message_index, parent.turns.slice(0, end))
    },
  }
}

interface s40_SessionCatalog requires root.ai.Provider {
  function list_sessions(self) -> s40_SessionInfo[] throws s40_SessionError | baml.errors.UnknownError
  function session_info(self, id: string) -> s40_SessionInfo throws s40_SessionError | baml.errors.UnknownError
  function session_messages(self, id: string) -> root.ai.ChatMessage[] throws s40_SessionError | baml.errors.UnknownError
  function session_conversation(self, id: string) -> root.ai.Conversation
      throws s40_SessionError | baml.errors.UnknownError {
    root.ai.conversation_from_messages(self.session_messages(id))
  }
  function session_token(self, id: string) -> root.ai.HarnessSessionToken?
      throws s40_SessionError | baml.errors.UnknownError { null }
  function rename_session(self, id: string, title: string) -> null throws s40_SessionError | baml.errors.UnknownError
  function tag_session(self, id: string, tag: string) -> null throws s40_SessionError | baml.errors.UnknownError
  function resolve_pickup(self, pickup: s40_Pickup) -> string throws s40_SessionError | baml.errors.UnknownError
  function run_with_session<T>(self, request: root.ai.Request<T>, session_id: string) -> T
    throws s40_SessionError | baml.errors.UnknownError

  function run_in_session<T>(self, request: root.ai.Request<T>, pickup: s40_Pickup) -> T
      throws s40_SessionError | baml.errors.UnknownError {
    self.run_with_session<T>(request, self.resolve_pickup(pickup))
  }
}

class s40_ScriptedCatalog {
  store: s40_MemoryStore,
  cwd: string,
  cwd_keyed: bool,
  reply_json: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s40-scripted-catalog" }
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements s40_SessionCatalog {
    function list_sessions(self) -> s40_SessionInfo[]
        throws s40_SessionError | baml.errors.UnknownError { self.store.visible(self.cwd, self.cwd_keyed) }
    function session_info(self, id: string) -> s40_SessionInfo
        throws s40_SessionError | baml.errors.UnknownError { self.store.require(id, self.cwd, self.cwd_keyed).info }
    function session_messages(self, id: string) -> root.ai.ChatMessage[]
        throws s40_SessionError | baml.errors.UnknownError { self.store.require(id, self.cwd, self.cwd_keyed).turns }
    function rename_session(self, id: string, title: string) -> null
        throws s40_SessionError | baml.errors.UnknownError {
      self.store.require(id, self.cwd, self.cwd_keyed).info.title = title;
      null
    }
    function tag_session(self, id: string, tag: string) -> null
        throws s40_SessionError | baml.errors.UnknownError {
      let info = self.store.require(id, self.cwd, self.cwd_keyed).info;
      if (!info.tags.includes(tag)) { let _ = info.tags.push(tag); }
      null
    }
    function resolve_pickup(self, pickup: s40_Pickup) -> string
        throws s40_SessionError | baml.errors.UnknownError {
      s40_resolve_pickup(self.store, self.cwd, self.cwd_keyed, pickup)
    }
    function run_with_session<T>(self, request: root.ai.Request<T>, session_id: string) -> T
        throws s40_SessionError | baml.errors.UnknownError {
      let value: T = baml.json.from_string<T>(self.reply_json) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["scripted session output parse failed"] },
      };
      self.store.append(
        session_id,
        self.cwd,
        self.cwd_keyed,
        request.messages().concat([root.ai.ChatMessage.assistant(self.reply_json)]),
      );
      value
    }
  }
}

// Live integration wrapper: storage stays local, while execution rebinds the
// same Request<T> to a real Generate provider chosen by the application.
class s40_LiveCatalog {
  store: s40_MemoryStore,
  cwd: string,
  inner: root.ai.Provider,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "s40-live-catalog" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements s40_SessionCatalog {
    function list_sessions(self) -> s40_SessionInfo[]
        throws s40_SessionError | baml.errors.UnknownError { self.store.visible(self.cwd, true) }
    function session_info(self, id: string) -> s40_SessionInfo
        throws s40_SessionError | baml.errors.UnknownError { self.store.require(id, self.cwd, true).info }
    function session_messages(self, id: string) -> root.ai.ChatMessage[]
        throws s40_SessionError | baml.errors.UnknownError { self.store.require(id, self.cwd, true).turns }
    function rename_session(self, id: string, title: string) -> null
        throws s40_SessionError | baml.errors.UnknownError {
      self.store.require(id, self.cwd, true).info.title = title;
      null
    }
    function tag_session(self, id: string, tag: string) -> null
        throws s40_SessionError | baml.errors.UnknownError {
      let info = self.store.require(id, self.cwd, true).info;
      if (!info.tags.includes(tag)) { let _ = info.tags.push(tag); }
      null
    }
    function resolve_pickup(self, pickup: s40_Pickup) -> string
        throws s40_SessionError | baml.errors.UnknownError {
      s40_resolve_pickup(self.store, self.cwd, true, pickup)
    }
    function run_with_session<T>(self, request: root.ai.Request<T>, session_id: string) -> T
        throws s40_SessionError | baml.errors.UnknownError {
      let value = root.ai.run<T>(request.for_provider(self.inner)) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["live session turn failed"] },
      };
      let encoded = baml.json.to_string(value) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["live session value encode failed"] },
      };
      self.store.append(
        session_id,
        self.cwd,
        true,
        request.messages().concat([root.ai.ChatMessage.assistant(encoded)]),
      );
      value
    }
  }
}

function s40_prompt_template(
  text: string,
  structured: bool,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    if (structured) {
      baml.llm.assemble_prompt_ast(["", "\n", ""], [text, ctx.output_format])
    } else {
      baml.llm.assemble_prompt_ast(["", ""], [text])
    }
  }
}
