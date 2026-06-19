// ============================================================================
// 40-harness-sessions / implement.baml   (std-lib / library-author code)
//
// "I want the agent to already know how to Read/Edit/Bash/Grep, and I want its
//  conversation to live somewhere I can inspect, resume, tag, and query."
//
// This scenario sits on top of TWO earlier ones and adds the harness-specific
// divergence the background §5 names:
//
//   * from 37-harness-basics: `ClaudeCode` is a `Provider` whose transport is a
//     subprocess (NOT HttpProvider), plus `Realtime` + `ControlPlane`.  We REUSE
//     those interfaces verbatim and only add the storage + toolset surface here.
//   * from 17-history-and-sessions: the `SessionStore` 4-method protocol
//     (load/save/append/clear over `Turn[]`).  The whole point of THIS scenario
//     is that a harness's store is not an in-process map — it is a real
//     filesystem object: JSONL under ~/.claude/projects/<encoded-cwd>/*.jsonl.
//
// The three things that are genuinely new here, all flagged in evaluation.md:
//   (A) the opinionated built-in toolset (Read/Edit/Bash/Grep/...) as DATA the
//       harness ships, that you allow/deny rather than define;
//   (B) a `SessionStore` whose backend is the on-disk JSONL tree, with the
//       cwd-encoding footgun written out, not hidden;
//   (C) a `SessionCatalog` capability (net-new) for the queryable helpers
//       (listSessions / getSessionMessages / tagSession / renameSession) and the
//       continue/resume/fork verbs, surfaced as a capability the harness
//       advertises — because a chat-model Provider has no such store to query.
// ============================================================================


// ----------------------------------------------------------------------------
// (A) The opinionated built-in toolset.  NET-NEW data, not a capability.
//
// Background §5: "You don't define these — you allow or deny them."  So the
// toolset is *shipped* by the harness as a fixed catalog of named tools, and the
// app's only lever is an allow/deny list.  We model the catalog as a static
// function returning canonical `Tool` values (the `Tool` type from
// _conventions.md), so the same values flow into BOTH the harness's spawn flags
// AND, if the host wants to host-dispatch, the `Tools.run_tools` path.
//
// The `parameters: type` field is the first-class `type` primitive — these are
// the actual input schemas the subprocess advertises for each built-in tool.
// ----------------------------------------------------------------------------

class ReadArgs  { path: string, offset: int?, limit: int? }
class EditArgs  { path: string, old: string, new: string, replace_all: bool? }
class WriteArgs { path: string, content: string }
class BashArgs  { command: string, timeout_ms: int? }
class GrepArgs  { pattern: string, path: string?, glob: string? }
class GlobArgs  { pattern: string, path: string? }
class LsArgs    { path: string }

// The shipped toolset.  This is the "Read/Edit/Write/Bash/Grep/Glob/Ls" common
// set the background lists.  A harness exposes it as a static so a driver can
// enumerate the built-ins WITHOUT spawning anything.
class BuiltinTools {
  function all() -> Tool[] {
    [
      Tool { name: "Read",  description: "Read a file from the workspace.",        parameters: reflect.type_of<ReadArgs>() },
      Tool { name: "Edit",  description: "Replace an exact string in a file.",     parameters: reflect.type_of<EditArgs>() },
      Tool { name: "Write", description: "Write (overwrite) a file.",              parameters: reflect.type_of<WriteArgs>() },
      Tool { name: "Bash",  description: "Run a shell command in the sandbox.",    parameters: reflect.type_of<BashArgs>() },
      Tool { name: "Grep",  description: "Search file contents by regex.",         parameters: reflect.type_of<GrepArgs>() },
      Tool { name: "Glob",  description: "Find files by glob pattern.",            parameters: reflect.type_of<GlobArgs>() },
      Tool { name: "Ls",    description: "List a directory.",                      parameters: reflect.type_of<LsArgs>() },
    ]
  }

  // Apply an allow/deny policy.  Background §3/§5: the lever is allow-or-deny,
  // never redefine.  `allow == null` means "all"; deny always wins.
  function resolve(allow: string[]?, deny: string[]?) -> Tool[] {
    let out: Tool[] = [];
    for (let t in BuiltinTools.all()) {
      let allowed: bool = if (allow == null) { true } else { string_in(allow, t.name) };
      let denied: bool  = if (deny == null)  { false } else { string_in(deny, t.name) };
      if (allowed && !denied) { out = out.append(t); }
    }
    out
  }
}


// ----------------------------------------------------------------------------
// (B) The on-disk JSONL store.  A `SessionStore` (REUSED protocol from 17)
//     whose backend is a real filesystem object.
//
// This is the load-bearing claim of the scenario: "a harness is a Provider whose
// transcript store is a real filesystem object."  We REUSE the exact 4-method
// `SessionStore` interface from 17-history-and-sessions, so a harness session and
// a chat session share one protocol.  What differs is entirely in the impl body:
// every method is `$rust_io_function` over the JSONL tree, and the *key
// derivation* (cwd -> encoded dir) is the footgun, written out below.
//
// We re-declare the interface signature here for a self-contained file; in the
// real tree it is `import`ed from scenario 17.  Turn / SessionStore are the same
// shapes.
// ----------------------------------------------------------------------------

// (re-declared from 17 for a self-contained scenario; identical shape)
enum Role { System, User, Assistant, Tool }
class Turn { role: Role, blocks: map<string, unknown>[] }   // blocks left opaque here; 17 owns the grammar

interface SessionStore {
  function load(self, id: string) -> Turn[] throws baml.errors.Io
  function save(self, id: string, turns: Turn[]) -> null throws baml.errors.Io
  function append(self, id: string, turns: Turn[]) -> null throws baml.errors.Io
  function clear(self, id: string) -> null throws baml.errors.Io
}

// Metadata a queryable store keeps PER session (background §5: getSessionInfo,
// tags, rename).  This is the row `listSessions` returns.
class SessionInfo {
  id: string
  cwd: string                 // the working directory this session is keyed under
  title: string?              // renameSession target
  tags: string[]              // tagSession target
  message_count: int
  updated_at: string          // ISO-8601, host clock
  parent_id: string?          // set if this session was forked from another
  branch_point: string?       // the message id the fork diverged at (Pi-style pointer)
}

// The on-disk store.  Its identity IS a directory derived from a cwd.  Closing
// the process does NOT lose it (unlike 17's MemoryStore) — but moving the project
// DOES orphan it, because the directory name is the encoded cwd.  That tradeoff
// is the whole "what's hard" of §5, and it is visible right here in `new`.
class JsonlSessionStore {
  root: string                // e.g. "~/.claude/projects"
  cwd: string                 // the working directory this store is scoped to
  _handle: $rust_type         // host-held dir handle / fd cache

  // The footgun, made explicit.  The on-disk directory is
  //   <root>/<encoded-cwd>/
  // where <encoded-cwd> is the ABSOLUTE cwd with every non-alphanumeric char
  // replaced by '-'.  Two different absolute paths => two different dirs => you
  // cannot find a session created elsewhere.  We surface the derivation as a pure
  // function so a test can assert it and a tool can warn on it.
  function encode_cwd(cwd: string) -> string { $rust_io_function }   // /a/b -> -a-b

  function dir_for(self) -> string {
    self.root + "/" + JsonlSessionStore.encode_cwd(self.cwd)
  }

  // Open a store for a cwd.  NOTE: this commits to the cwd-keyed layout.  The
  // escape hatch (§5 "what's hard": a SessionStore adapter must exist from day
  // one) is simply to construct a DIFFERENT `implements SessionStore` (e.g. the
  // SqliteSessionStore below) and hand THAT to the harness instead.
  function open(root: string, cwd: string) -> JsonlSessionStore {
    JsonlSessionStore { root: root, cwd: cwd, _handle: $rust_io_function }
  }

  implements SessionStore {
    // load: read <dir>/<id>.jsonl, parse each newline-delimited record into a
    // Turn.  The "content blocks buried in nested types" hazard (§5 what's hard:
    // SDKAssistantMessage.message is a BetaMessage) is absorbed HERE — the host
    // body flattens the nested Anthropic content blocks into our flat Turn.blocks.
    function load(self, id: string) -> Turn[] throws baml.errors.Io { $rust_io_function }

    // save: rewrite the whole <id>.jsonl.  Used by compaction (§5: a
    // compact_boundary rewrite is lossy) — save replaces history wholesale.
    function save(self, id: string, turns: Turn[]) -> null throws baml.errors.Io { $rust_io_function }

    // append: the hot path.  ONE fsync-append per turn pair; this is what makes
    // the transcript debuggable mid-run (you can `tail -f` the jsonl).
    function append(self, id: string, turns: Turn[]) -> null throws baml.errors.Io { $rust_io_function }

    // clear: unlink <id>.jsonl.
    function clear(self, id: string) -> null throws baml.errors.Io { $rust_io_function }
  }
}

// The day-one escape hatch the background demands: a NON-cwd-keyed store with the
// SAME protocol, so resume works after `mv`.  Keyed by explicit id, not cwd.
// Proves the footgun is a property of ONE implementation, not the seam.
class SqliteSessionStore {
  db_path: string
  _conn: $rust_type

  function open(db_path: string) -> SqliteSessionStore {
    SqliteSessionStore { db_path: db_path, _conn: $rust_io_function }
  }

  implements SessionStore {
    function load(self, id: string) -> Turn[] throws baml.errors.Io { $rust_io_function }
    function save(self, id: string, turns: Turn[]) -> null throws baml.errors.Io { $rust_io_function }
    function append(self, id: string, turns: Turn[]) -> null throws baml.errors.Io { $rust_io_function }
    function clear(self, id: string) -> null throws baml.errors.Io { $rust_io_function }
  }
}


// ----------------------------------------------------------------------------
// (C) The queryable + pick-up-history surface.  NET-NEW capability:
//     `SessionCatalog requires Provider`.
//
// Why a capability and not just methods on JsonlSessionStore?  Because the app
// drives a *Provider* (the harness), not a store directly.  Background §5's
// helpers (listSessions / getSessionMessages / getSessionInfo / renameSession /
// tagSession) plus the pickup verbs (continue / resume / fork) are things you ask
// the *harness* to do — and a plain chat-model Provider has no such store, so it
// must be negotiable like every other capability (proposal §5: capability is a
// runtime `match`).
//
// `SessionCatalog` is the harness analog of 17's `Conversational`: it threads a
// persisted transcript through `Provider.call`, but here the transcript is on
// disk and addressable by id, and the pickup mode (continue/resume/fork) selects
// WHICH transcript and HOW to diverge.
// ----------------------------------------------------------------------------

// How to pick up prior history.  Mirrors the Claude SDK Options (§5) but as a
// closed union instead of a bag of booleans — so "continue AND resume" can't be
// expressed (it's nonsense in the SDK too, just not prevented there).
class Fresh    {}                                              // brand-new session, auto id
class Continue {}                                              // most recent session in THIS cwd
class Resume   { session_id: string }                         // a specific id
class ResumeAt { session_id: string, message_id: string }     // resume at a specific message
class Fork     { session_id: string, at_message: string? }    // copy/branch into a NEW id

type Pickup = Fresh | Continue | Resume | ResumeAt | Fork

interface SessionCatalog requires Provider {
  // The store this catalog reads/writes.  Associated so a harness can bind its
  // own (JSONL) while the protocol stays storage-agnostic.
  type Store: SessionStore

  // --- queryable helpers (operate on the store WITHOUT running the agent) ---
  function list_sessions(self) -> SessionInfo[] throws baml.errors.Io
  function session_info(self, id: string) -> SessionInfo throws baml.errors.Io
  function session_messages(self, id: string) -> Turn[] throws baml.errors.Io
  function rename_session(self, id: string, title: string) -> null throws baml.errors.Io
  function tag_session(self, id: string, tag: string) -> null throws baml.errors.Io

  // --- pickup: resolve a Pickup into a concrete, writable session id ---
  // Fork is where the storage models diverge (background §5: Claude copies the
  // prefix, Pi points at the parent).  We expose the SEMANTIC ("give me a new id
  // seeded from this history") and let the impl choose copy-vs-pointer.
  function resolve_pickup(self, p: Pickup) -> string throws baml.errors.Io

  // --- the threaded run: pick up history, run one turn, persist back ---
  // This is the harness analog of 17.Conversational.chat: it bottoms out on the
  // provider's own `call`, but loads/appends the on-disk transcript around it.
  function call_in_session<T>(self, prompt: baml.llm.PromptAst, p: Pickup) -> T throws Error {
    let id: string = self.resolve_pickup(p) catch (e) {
      _ => { throw baml.errors.LlmClient { message: "could not resolve session pickup" }; }
    };
    // The harness's own call runs the turn (subprocess); the host body of
    // `call_with_session` is responsible for pointing the subprocess at session
    // `id` so the subprocess appends to the SAME jsonl.  We pass the id as a
    // structured directive the provider understands.
    self.call_with_session<T>(prompt, id)
  }

  // The provider-specific bridge: run `call`, but bound to an explicit session id
  // so persistence lands in the right jsonl.  A harness implements this with its
  // subprocess; nothing portable can be said about it (Open Q3/Q7).
  function call_with_session<T>(self, prompt: baml.llm.PromptAst, session_id: string) -> T throws Error
}


// ----------------------------------------------------------------------------
// (D) The harness provider, extended.
//
// We re-declare a `ClaudeCode` matching 37's fields + the storage surface this
// scenario adds.  In the real tree this is the SAME class as 37's; here it gains
// `session_root` and the `SessionCatalog` + `Tools` impls wired to the JSONL
// store and the built-in toolset.  Realtime/ControlPlane are unchanged from 37
// (elided to a comment to avoid re-teaching them).
// ----------------------------------------------------------------------------

enum Sandbox { Local, Container, Virtual }
enum PermissionMode { Default, AcceptEdits, Plan, BypassPermissions }
class HarnessTranscript { user_message_ids: string[], final_text: string, _raw: $rust_type }

class ClaudeCode {
  model: string?
  sandbox: Sandbox
  permission_mode: PermissionMode
  allowed_tools: string[]?
  disallowed_tools: string[]?
  cwd: string                          // REQUIRED here: it is the session key (§5)
  session_root: string                 // e.g. "~/.claude/projects"

  function in_dir(dir: string) -> ClaudeCode {
    ClaudeCode {
      model: null,
      sandbox: Sandbox.Container,
      permission_mode: PermissionMode.AcceptEdits,
      allowed_tools: ["Read", "Edit", "Bash", "Grep", "Glob"],
      disallowed_tools: null,
      cwd: dir,
      session_root: "~/.claude/projects",
    }
  }

  // The cwd-keyed store this harness reads/writes.  This single line is where the
  // footgun is committed: the store identity is `(session_root, cwd)`.
  function store(self) -> JsonlSessionStore {
    JsonlSessionStore.open(self.session_root, self.cwd)
  }

  // The effective toolset, after allow/deny resolution.  An app or driver can
  // call this to render its permission UI without spawning the subprocess.
  function tools(self) -> Tool[] {
    BuiltinTools.resolve(self.allowed_tools, self.disallowed_tools)
  }

  // -- base Provider: one-shot, fresh session (37, unchanged) -----------------
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function   // spawn `claude --print` in self.sandbox, SAP-parse final
    }
  }

  // -- Realtime + ControlPlane: identical to 37-harness-basics; the driven,
  //    steerable session.  Omitted here to keep this file about STORAGE; the
  //    control plane (Steer/FollowUp/Interrupt/RewindFiles/...) is unchanged.
  // implements Realtime    { ... }   // see 37
  // implements ControlPlane { ... }  // see 37

  // -- SessionCatalog: the queryable store + continue/resume/fork --------------
  implements SessionCatalog {
    type Store = JsonlSessionStore

    // Every helper delegates to the host, scoped to THIS cwd's directory.  The
    // cwd-coupling is visible: list_sessions can only ever see sessions under
    // self.cwd — move the project and this returns [].
    function list_sessions(self) -> SessionInfo[] throws baml.errors.Io {
      // scans <session_root>/<encode_cwd(cwd)>/*.jsonl, reading the header record
      list_sessions_in_dir(self.store().dir_for())
    }
    function session_info(self, id: string) -> SessionInfo throws baml.errors.Io {
      session_info_in_dir(self.store().dir_for(), id)
    }
    function session_messages(self, id: string) -> Turn[] throws baml.errors.Io {
      // reuse the store's own load — same jsonl, same flattening of nested blocks
      self.store().load(id)
    }
    function rename_session(self, id: string, title: string) -> null throws baml.errors.Io {
      rename_session_in_dir(self.store().dir_for(), id, title)
    }
    function tag_session(self, id: string, tag: string) -> null throws baml.errors.Io {
      tag_session_in_dir(self.store().dir_for(), id, tag)
    }

    // resolve_pickup: the continue/resume/fork verbs.  This is where the
    // copy-vs-pointer fork divergence lives.  Written out, not elided, because it
    // is the teaching point of "fork cost varies".
    function resolve_pickup(self, p: Pickup) -> string throws baml.errors.Io {
      let dir: string = self.store().dir_for();
      match (p) {
        let f: Fresh    => { return new_session_id(dir); },             // mint a fresh jsonl
        let c: Continue => {
          // most-recently-updated session in THIS cwd.  Empty if none / moved dir.
          let infos: SessionInfo[] = list_sessions_in_dir(dir);
          if (infos.len() == 0) { return new_session_id(dir); }         // nothing to continue -> fresh
          return most_recent(infos).id;
        },
        let r: Resume   => { return r.session_id; },                    // caller named it
        let ra: ResumeAt => {
          // Claude SDK resumeSessionAt: truncate the transcript at message_id,
          // keep the SAME id (history after that point is discarded on next save).
          truncate_session_at(dir, ra.session_id, ra.message_id);
          return ra.session_id;
        },
        let fk: Fork    => {
          // Fork-as-COPY (Claude semantics): mint a new id, copy the prefix up to
          // at_message (or the whole thing) into it, record parent + branch_point.
          // A Pi-style harness would instead write a pointer record and copy
          // nothing — same SEMANTIC, cheaper storage.  The choice is the impl's.
          let new_id: string = new_session_id(dir);
          fork_copy_prefix(dir, fk.session_id, new_id, fk.at_message);  // host: copy or pointer
          return new_id;
        },
      }
    }

    // call_with_session: subprocess bound to an explicit jsonl.  The host body
    // points `claude --resume <id>` at the right file so its own appends land in
    // OUR store (the subprocess and BAML must agree on the file — the load-bearing
    // coupling, Open Q3/Q7).
    function call_with_session<T>(self, prompt: baml.llm.PromptAst, session_id: string) -> T throws baml.errors.LlmClient {
      $rust_io_function
    }
  }

  // -- Tools: host-dispatch of the SAME built-in toolset (37, with toolset wired).
  // When the host wants its own dispatch policy, the harness surfaces the built-in
  // tools as external calls.  `begin` seeds with self.tools() so allow/deny is
  // honored even on the host-dispatch path.
  implements Tools {
    type Transcript = HarnessTranscript
    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> HarnessTranscript throws baml.errors.LlmClient {
      // tools passed by the host are MERGED with (or override) self.tools();
      // host body decides precedence.  id correlation is the subprocess's, opaque.
      $rust_io_function
    }
    function step<T>(self, t: HarnessTranscript) -> T | ToolCalls throws baml.errors.LlmClient { $rust_io_function }
    function submit(self, t: HarnessTranscript, results: ToolResult[]) -> HarnessTranscript throws baml.errors.LlmClient { $rust_io_function }
    // run_tools<T> inherited from Tools default.
  }
}


// ----------------------------------------------------------------------------
// (E) A divergent second harness: Pi, with a session TREE instead of a flat
//     cwd-keyed dir.  Same `SessionCatalog` interface; fork is a POINTER, and
//     there is no cwd coupling (Pi keys on an explicit id).  This is the
//     portability proof: the queryable surface holds across two real storage
//     substrates, and the divergence (copy vs pointer, cwd vs id) is confined to
//     resolve_pickup + the store impl.
// ----------------------------------------------------------------------------

class PiAgent {
  model: string
  cwd: string?
  tree_db: string                      // Pi's session-tree store path (NOT cwd-keyed)
  _tree: $rust_type

  function open(model: string, tree_db: string) -> PiAgent {
    PiAgent { model: model, cwd: null, tree_db: tree_db, _tree: $rust_io_function }
  }

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient { $rust_io_function }
  }

  implements SessionCatalog {
    // Pi's store is a tree DB, but it STILL satisfies the 4-method SessionStore
    // protocol (load/save/append/clear over a leaf id).
    type Store = PiTreeStore

    function list_sessions(self) -> SessionInfo[] throws baml.errors.Io {
      pi_tree_list(self._tree)                       // walks leaves; no cwd filter
    }
    function session_info(self, id: string) -> SessionInfo throws baml.errors.Io {
      pi_tree_info(self._tree, id)
    }
    function session_messages(self, id: string) -> Turn[] throws baml.errors.Io {
      // a leaf's transcript is its prefix walked from the root — the pointer
      // model: reading message history MATERIALIZES the chain, it isn't stored flat.
      pi_tree_materialize(self._tree, id)
    }
    function rename_session(self, id: string, title: string) -> null throws baml.errors.Io {
      pi_tree_rename(self._tree, id, title)
    }
    function tag_session(self, id: string, tag: string) -> null throws baml.errors.Io {
      pi_tree_tag(self._tree, id, tag)
    }

    function resolve_pickup(self, p: Pickup) -> string throws baml.errors.Io {
      match (p) {
        let f: Fresh    => { return pi_tree_new_root(self._tree); },
        let c: Continue => {
          // Pi has NO cwd key; "continue" means "the most recent leaf overall".
          let infos: SessionInfo[] = pi_tree_list(self._tree);
          if (infos.len() == 0) { return pi_tree_new_root(self._tree); }
          return most_recent(infos).id;
        },
        let r: Resume   => { return r.session_id; },
        let ra: ResumeAt => {
          // In a tree, "resume at message" is just branching at that node — there
          // is no in-place truncation. ResumeAt collapses to Fork-without-copy.
          return pi_tree_branch_at(self._tree, ra.session_id, ra.message_id);
        },
        let fk: Fork    => {
          // ★ DIVERGENCE vs ClaudeCode: fork is a POINTER, not a copy. A new leaf
          // points at the branch node as parent; NO prefix is copied.  Same
          // Pickup variant, O(1) instead of O(history).
          return pi_tree_branch_at(self._tree, fk.session_id, optional_or(fk.at_message, pi_tree_head(self._tree, fk.session_id)));
        },
      }
    }

    function call_with_session<T>(self, prompt: baml.llm.PromptAst, session_id: string) -> T throws baml.errors.LlmClient {
      $rust_io_function
    }
  }
}

// Pi's tree store as a SessionStore — proves the 4-method protocol survives a
// pointer-based substrate (load = materialize the chain).
class PiTreeStore {
  _tree: $rust_type
  implements SessionStore {
    function load(self, id: string) -> Turn[] throws baml.errors.Io { pi_tree_materialize(self._tree, id) }
    function save(self, id: string, turns: Turn[]) -> null throws baml.errors.Io { $rust_io_function }
    function append(self, id: string, turns: Turn[]) -> null throws baml.errors.Io { $rust_io_function }
    function clear(self, id: string) -> null throws baml.errors.Io { $rust_io_function }
  }
}


// ----------------------------------------------------------------------------
// (F) Host boundaries + pure helpers.
//     The JSONL/tree operations are genuine host I/O ($rust_io_function); the
//     small list/string utilities are pure.
// ----------------------------------------------------------------------------

// --- JSONL store host ops (cwd-keyed dir) ---
function list_sessions_in_dir(dir: string) -> SessionInfo[] throws baml.errors.Io { $rust_io_function }
function session_info_in_dir(dir: string, id: string) -> SessionInfo throws baml.errors.Io { $rust_io_function }
function rename_session_in_dir(dir: string, id: string, title: string) -> null throws baml.errors.Io { $rust_io_function }
function tag_session_in_dir(dir: string, id: string, tag: string) -> null throws baml.errors.Io { $rust_io_function }
function new_session_id(dir: string) -> string throws baml.errors.Io { $rust_io_function }
function truncate_session_at(dir: string, id: string, message_id: string) -> null throws baml.errors.Io { $rust_io_function }
function fork_copy_prefix(dir: string, from_id: string, to_id: string, at_message: string?) -> null throws baml.errors.Io { $rust_io_function }

// --- Pi tree host ops (pointer model) ---
function pi_tree_list(tree: $rust_type) -> SessionInfo[] throws baml.errors.Io { $rust_io_function }
function pi_tree_info(tree: $rust_type, id: string) -> SessionInfo throws baml.errors.Io { $rust_io_function }
function pi_tree_materialize(tree: $rust_type, id: string) -> Turn[] throws baml.errors.Io { $rust_io_function }
function pi_tree_rename(tree: $rust_type, id: string, title: string) -> null throws baml.errors.Io { $rust_io_function }
function pi_tree_tag(tree: $rust_type, id: string, tag: string) -> null throws baml.errors.Io { $rust_io_function }
function pi_tree_new_root(tree: $rust_type) -> string throws baml.errors.Io { $rust_io_function }
function pi_tree_branch_at(tree: $rust_type, id: string, message_id: string) -> string throws baml.errors.Io { $rust_io_function }
function pi_tree_head(tree: $rust_type, id: string) -> string throws baml.errors.Io { $rust_io_function }

// --- pure helpers ---
function string_in(xs: string[], needle: string) -> bool {
  for (let x in xs) { if (x == needle) { return true; } }
  false
}

function most_recent(infos: SessionInfo[]) -> SessionInfo {
  let best: SessionInfo = infos[0];
  for (let i in infos) { if (i.updated_at > best.updated_at) { best = i; } }
  best
}

function optional_or(v: string?, fallback: string) -> string {
  match (v) { let s: string => s, _ => fallback }
}
