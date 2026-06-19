// ============================================================================
// 40-harness-sessions / usage.baml   (APPLICATION-author code)
//
// What an app author writes to: ship the opinionated toolset, run with an
// on-disk session, and continue / resume / fork it — plus call the queryable
// helpers (listSessions / tagSession) WITHOUT running the agent.
//
// The ergonomics claim: the client stays config-only (cwd + session_root are
// just fields, like `model`), and the session pickup is a parameter handed to a
// companion at the call — never baked into the client.  Querying the store is a
// capability `match`, exactly like `.stream` / `.live`.
// ============================================================================


// ---- clients are config-only; cwd is the session key ------------------------

client CodeAgent() {
  ClaudeCode {
    model: "claude-sonnet-4-6",
    sandbox: Sandbox.Container,
    permission_mode: PermissionMode.AcceptEdits,
    allowed_tools: ["Read", "Edit", "Bash", "Grep", "Glob"],   // the opinionated set
    disallowed_tools: ["Write"],                               // deny: read-mostly agent
    cwd: "/workspace/project",                                 // ← the session key (§5 footgun)
    session_root: "~/.claude/projects",
  }
}

// Same harness, DIFFERENT cwd => a DIFFERENT session directory.  This is the
// footgun in the open: `CodeAgent()` and `OtherDir()` cannot see each other's
// sessions even though they are the same model + tools.
client OtherDir() {
  ClaudeCode {
    model: "claude-sonnet-4-6",
    sandbox: Sandbox.Container,
    permission_mode: PermissionMode.AcceptEdits,
    allowed_tools: null,
    disallowed_tools: null,
    cwd: "/workspace/other",
    session_root: "~/.claude/projects",
  }
}

// The escape hatch from day one: a harness whose store is NOT cwd-keyed.  We
// can't express "swap the store" as a field on ClaudeCode (its store() is
// hardwired to JSONL), so the honest move is a SECOND harness — Pi — whose store
// is an explicit-id tree.  Resume survives `mv` here.
client PortableCoder() {
  PiAgent.open("anthropic/claude-sonnet-4-6", "~/.pi/sessions.db")
}


// ---- one-shot: fresh session every call (the base `.call`) -------------------

class Patch { files_changed: string[], summary: string }

function Fix(task: string) -> Patch {
  client: CodeAgent()
  prompt #"Fix the bug: {{ task }}"#
}
// Fix("auth null deref")  ->  ClaudeCode.call<Patch>(prompt)  -> writes a NEW jsonl.


// ---- continue / resume / fork: a SessionCatalog companion --------------------
//
// These are NOT on the plain `Fix(...)` call.  The pickup is a parameter of a
// catalog companion, the same way `io` is a parameter of `.live`.  We write the
// companion longhand to show the negotiation an app author does (and to show the
// `_` arm: a chat model has no store, so this errors honestly).

function FixContinuing(task: string, pickup: Pickup) -> Patch throws baml.errors.LlmClient {
  let p = CodeAgent();
  match (p) {
    let cat: SessionCatalog => cat.call_in_session<Patch>(render_prompt(task), pickup),
    _ => throw baml.errors.Unsupported { message: "client has no on-disk session store" },
  }
}
// continue the most recent session in this cwd:
//   FixContinuing("now add a test", Continue {})
// resume a specific id:
//   FixContinuing("keep going", Resume { session_id: "9b2f-..." })
// fork at a known checkpoint and diverge (Claude copies the prefix; Pi points):
//   FixContinuing("try a different approach", Fork { session_id: "9b2f-...", at_message: "msg_12" })


// ---- queryable helpers: operate on the store WITHOUT running the agent -------
//
// listSessions / getSessionMessages / tagSession / renameSession from §5.  This
// is the "the transcript is a real store" payoff: you inspect and curate history
// without spawning anything.  Again gated by a capability `match` — a chat client
// would simply not be a SessionCatalog.

function TriageSessions(incident_tag: string) -> SessionInfo[] throws baml.errors.LlmClient {
  let p = CodeAgent();
  match (p) {
    let cat: SessionCatalog => {
      let sessions: SessionInfo[] = cat.list_sessions() catch (e) {
        _ => { throw baml.errors.LlmClient { message: "session store unreadable" }; }
      };
      // tag the most recent few so an on-call engineer can find them later.
      let recent: SessionInfo[] = [];
      for (let s in sessions) {
        if (s.message_count > 0) {
          cat.tag_session(s.id, incident_tag) catch (e) { _ => {} };
          cat.rename_session(s.id, "triage: " + optional_or(s.title, s.id)) catch (e) { _ => {} };
          recent = recent.append(s);
        }
      }
      return recent;
    },
    _ => throw baml.errors.Unsupported { message: "client is not queryable" },
  }
}

// Pull the raw transcript of one session for an audit view — note this reaches
// the SAME jsonl the agent appends to, so it reflects in-flight state.
function AuditSession(session_id: string) -> Turn[] throws baml.errors.LlmClient {
  let p = CodeAgent();
  match (p) {
    let cat: SessionCatalog => cat.session_messages(session_id),
    _ => throw baml.errors.Unsupported { message: "client is not queryable" },
  }
}


// ---- the footgun, demonstrated ----------------------------------------------
//
// Continue against the RIGHT cwd works; against the wrong cwd silently starts
// fresh (because list_sessions in that dir is empty).  An app author who moved
// the project gets a brand-new session and no error — the §5 "orphaned
// transcripts" hazard, reproduced honestly.

function ContinueHere(task: string) -> Patch throws baml.errors.LlmClient {
  // Resolves to the most recent session under "/workspace/project".
  FixContinuing(task, Continue {})
}

function ContinueElsewhere(task: string) -> Patch throws baml.errors.LlmClient {
  let p = OtherDir();      // cwd = /workspace/other => a DIFFERENT encoded dir
  match (p) {
    let cat: SessionCatalog => cat.call_in_session<Patch>(render_prompt(task), Continue {}),
    _ => throw baml.errors.Unsupported { message: "no store" },
  }
  // If you MEANT to continue /workspace/project's session, this silently starts
  // a fresh one. No type, no runtime error catches it. That is the footgun.
}


// ---- the day-one escape hatch: same code, survives a moved project ----------
//
// PortableCoder keys on an explicit id, not cwd. The SAME companion compiles and
// runs against it because PiAgent is also a SessionCatalog — proving the
// queryable surface is portable and the cwd-coupling was an impl property.

function FixPortable(task: string, pickup: Pickup) -> Patch throws baml.errors.LlmClient {
  let p = PortableCoder();
  match (p) {
    let cat: SessionCatalog => cat.call_in_session<Patch>(render_prompt(task), pickup),
    _ => throw baml.errors.Unsupported { message: "client has no session store" },
  }
}
// FixPortable("resume after I moved the repo", Resume { session_id: "leaf-77" })  -> still works.


// ---- inspect the shipped toolset without spawning ---------------------------
//
// `ClaudeCode.tools()` resolves allow/deny up front, so a permission UI can be
// rendered before the agent runs. Pure data, no capability needed.

function ToolPalette() -> Tool[] {
  let p = CodeAgent();
  // p is the existential Provider; to read tools() we need the concrete type.
  // This is the compile-time-precision escape hatch (proposal §3/§5): if we want
  // tools() statically, we drop the `client` sugar. Here we just match.
  match (p) {
    let c: SessionCatalog => {
      // SessionCatalog doesn't expose tools(); tools() is a ClaudeCode method.
      // To read it we'd need the concrete class — see evaluation (c). For the
      // built-in catalog itself we can go straight to the static:
      BuiltinTools.resolve(["Read", "Edit", "Bash", "Grep", "Glob"], ["Write"])
    },
    _ => BuiltinTools.all(),
  }
}
