// =============================================================================
// 21-memory — implement.baml  (std-lib / library-author code)
//
// The teaching point: "Memory" is NOT a Provider. A Provider produces a typed
// answer from a prompt. A memory layer stores and recalls *facts*. They have
// different shapes, so memory is its own interface — and the proposal's
// Provider/capability machinery shows up in three distinct places:
//
//   1. The vector DB = the disk        -> a plain host-backed store (KV-ish).
//   2. The memory layer = the filesystem -> MemoryStore interface, with the
//      LLM-curated (mem0) and raw (LangGraph BaseStore) implementations.
//   3. Wiring memory into a call = TWO postures, both expressed with the
//      canonical primitives:
//        (a) middleware: a Combinator that wraps a Provider (retrieve-before /
//            store-after) — exactly like Retry/Cache in the spine.
//        (b) tool: memory exposed as a `Tool` so the *model* drives recall, and
//            the Anthropic memory-tool case where the model drives the *whole*
//            filesystem (view/create/str_replace/...) through the Tools loop.
//
// We build ON the spine (Provider, HttpProvider, Streaming, Tools, the Cache
// combinator pattern). Everything below the $rust_io_function boundary is the
// vector DB / embedding host; everything above is BAML.
// =============================================================================


// -----------------------------------------------------------------------------
// NET-NEW capability-error interface. MemoryStore is a sibling capability (not
// call/stream/realtime/tools), so under the error model it needs its OWN
// capability-error interface with the same classifier shape, and its methods
// declare `baml.ExtendUnknownError<baml.errors.MemoryError>`. Flagged net-new in
// evaluation.md. The disk below it (VectorStore) is the host boundary and keeps
// raising the foreign `baml.errors.Io`; the layer normalizes that into its own
// channel.
// -----------------------------------------------------------------------------
interface baml.errors.MemoryError {
  function is_network_error(self) -> bool   // the disk/embedding host was unreachable
  function is_not_found(self) -> bool       // a scoped key/row did not exist
}

// Read a required string argument out of a ToolCall's `args` (a map<string,
// unknown>): look the key up with the real Map.get method and narrow the
// `unknown?` it returns to a string. A missing/ill-typed arg is a protocol
// violation, surfaced as Unsupported (legal on any capability channel).
function arg_str(args: map<string, unknown>, key: string) -> string
    throws baml.errors.Unsupported {
  match (args.get(key)) {
    let s: string => s,
    _ => throw baml.errors.Unsupported { message: "tool arg missing or not a string: " + key },
  }
}


// -----------------------------------------------------------------------------
// 0. Scope — mem0's set-theoretic identifiers, LangGraph's namespace tuples, and
//    ADK's prefix strings all reduce to "a partition key with optional facets".
//    We model it as a small record so an impl can index on whatever subset it
//    supports. NET-NEW host primitive: nothing — it's a plain class.
// -----------------------------------------------------------------------------
class Scope {
  user_id: string?      // mem0 user_id  / LangGraph ("user", id, ...) / ADK user:
  agent_id: string?     // mem0 agent_id / persona-scoped
  run_id: string?       // mem0 run_id   / one conversation
  app_id: string?       // mem0 app_id   / ADK app:
}

// A single stored fact. `id` is the dedup/update handle (mem0 preserves it on
// UPDATE, changes the text). `created_at`/`updated_at` are host timestamps.
class Memory {
  id: string
  memory: string                       // third-person declarative ("User is vegetarian")
  metadata: map<string, string>
  score: float?                        // populated only by search()
  created_at: string
  updated_at: string
}

// The extract->decide vocabulary. This is the entire reason a memory LAYER
// exists instead of a raw vector upsert. NONE = "already covered, do nothing".
enum MemoryOp { ADD, UPDATE, DELETE, NONE }

class MemoryDecision {
  op: MemoryOp
  id: string?                          // present for UPDATE/DELETE (which row)
  text: string?                        // present for ADD/UPDATE (the new text)
}

// A turn handed to add() for extraction. Mirrors the {role, content} the
// background shows mem0/Anthropic consuming.
class Turn { role: string, content: string }


// -----------------------------------------------------------------------------
// 1. The DISK — a vector store. Raw upsert/query, no judgment. This is the
//    Pinecone/Qdrant/pgvector layer: it knows vectors, not facts.
//    NET-NEW host primitives: the embedding + vector-search host functions.
// -----------------------------------------------------------------------------
class VectorRow { id: string, text: string, metadata: map<string, string>, score: float? }

interface VectorStore {
  // embed-then-upsert is two steps so a layer can reuse the embedding.
  function upsert(self, id: string, vector: float[], text: string, md: map<string, string>) -> null throws baml.errors.Io
  function query(self, vector: float[], top_k: int, filter: map<string, string>) -> VectorRow[] throws baml.errors.Io
  function delete(self, id: string) -> null throws baml.errors.Io
  function embed(self, text: string) -> float[] throws baml.errors.Io
}

// Qdrant-backed concrete disk. The host owns the wire; BAML owns nothing here
// except the shape. This is the "$rust_io_function like PrimitiveClient" bridge
// (Open Question 7).
class Qdrant {
  url: string
  collection: string
  embed_model: string

  implements VectorStore {
    function upsert(self, id: string, vector: float[], text: string, md: map<string, string>) -> null throws baml.errors.Io {
      $rust_io_function
    }
    function query(self, vector: float[], top_k: int, filter: map<string, string>) -> VectorRow[] throws baml.errors.Io {
      $rust_io_function
    }
    function delete(self, id: string) -> null throws baml.errors.Io {
      $rust_io_function
    }
    function embed(self, text: string) -> float[] throws baml.errors.Io {
      $rust_io_function   // calls embed_model
    }
  }
}


// -----------------------------------------------------------------------------
// 2. The FILESYSTEM — a memory layer. THIS is the abstraction teams actually
//    swap (mem0 vs Zep vs LangGraph store vs Letta archival). It is NOT a
//    Provider; it is its own interface. The whole "no standard / swapping is a
//    rewrite" pain from the background is exactly the question of whether this
//    interface is wide enough.
//
//    Primary operations are search (retrieve-before) and add (store-after).
//    add() is where curation lives — and its body is the divergence point.
// -----------------------------------------------------------------------------
interface MemoryStore {
  // retrieve-before: relevance search scoped by identifiers.
  function search(self, query: string, scope: Scope, top_k: int) -> Memory[]
      throws baml.ExtendUnknownError<baml.errors.MemoryError>

  // store-after: hand the raw exchange; the layer decides what (if anything) to
  // keep. Returns the decisions it applied (so a caller can audit/log).
  function add(self, turns: Turn[], scope: Scope) -> MemoryDecision[]
      throws baml.ExtendUnknownError<baml.errors.MemoryError>

  // direct mutation — used by Letta self-editing tools and the raw BaseStore.
  function put(self, scope: Scope, key: string, value: string) -> null
      throws baml.ExtendUnknownError<baml.errors.MemoryError>
  function get_all(self, scope: Scope) -> Memory[]
      throws baml.ExtendUnknownError<baml.errors.MemoryError>
}


// -----------------------------------------------------------------------------
// 2a. mem0 — IMPLICIT curation. add() runs the extract->retrieve->decide->persist
//     LLM pipeline. Note the two LLM calls are PROVIDERS handed in as fields:
//     this is where the proposal's "everything is a Provider" pays off — the
//     memory layer is a *consumer* of providers, not a provider itself.
//
//     Under the marker spine (../provider-as-marker.md) `call<T>` lives on
//     HttpProvider, not on the bare Provider marker, so the two LLMs the pipeline
//     calls are typed `HttpProvider` (request/response chat models) — that is the
//     capability `add()` actually uses. They are still Providers (HttpProvider
//     requires Provider); the field just names the capability it needs.
// -----------------------------------------------------------------------------
class Mem0 {
  disk: VectorStore        // the raw vector DB (the "disk")
  extractor: HttpProvider  // LLM that emits candidate facts
  decider: HttpProvider    // LLM that classifies ADD/UPDATE/DELETE/NONE

  // Build the partition filter the disk understands from the scope record.
  function scope_filter(scope: Scope) -> map<string, string> {
    let f: map<string, string> = {};
    match (scope.user_id)  { let v: string => { f.set("user_id", v); }, _ => {} }
    match (scope.agent_id) { let v: string => { f.set("agent_id", v); }, _ => {} }
    match (scope.run_id)   { let v: string => { f.set("run_id", v); }, _ => {} }
    match (scope.app_id)   { let v: string => { f.set("app_id", v); }, _ => {} }
    f
  }

  implements MemoryStore {
    function search(self, query: string, scope: Scope, top_k: int) -> Memory[]
        throws baml.ExtendUnknownError<baml.errors.MemoryError> {
      let v = self.disk.embed(query);
      let rows = self.disk.query(v, top_k, Mem0.scope_filter(scope));
      let out: Memory[] = [];
      for (let r in rows) {
        out.push(Memory {
          id: r.id, memory: r.text, metadata: r.metadata, score: r.score,
          created_at: "", updated_at: "",
        });
      }
      out
    } catch (e) {
      // foreign disk/embedding Io normalized into the MemoryError channel.
      _ => throw baml.UnknownError.with_message<baml.errors.MemoryError>(e, "mem0 search failed");
    }

    // The headline pipeline. Each step is a provider .call<T> — the four-stage
    // extract->retrieve->decide->persist made of ordinary typed calls.
    function add(self, turns: Turn[], scope: Scope) -> MemoryDecision[]
        throws baml.ExtendUnknownError<baml.errors.MemoryError> {
      // (1) EXTRACT — LLM emits candidate facts as short declarative statements.
      let transcript = Mem0.render_turns(turns);
      let candidates: string[] = self.extractor.call<string[]>(
        baml.llm.render("Extract durable, third-person facts worth remembering from:\n{{t}}", { t: transcript })
      );

      let applied: MemoryDecision[] = [];
      for (let cand in candidates) {
        // (2) RETRIEVE NEIGHBORS — same-scope top-K for contradiction context.
        let vec = self.disk.embed(cand);
        let neighbors = self.disk.query(vec, 5, Mem0.scope_filter(scope));

        // (3) DECIDE — the genuinely hard call: ADD/UPDATE/DELETE/NONE.
        let decision: MemoryDecision = self.decider.call<MemoryDecision>(
          baml.llm.render(
            "New fact: {{c}}\nExisting nearby memories:\n{{n}}\n" +
            "Decide ADD (new), UPDATE (rewrite an existing id), DELETE (contradicts), or NONE (covered).",
            { c: cand, n: Mem0.render_rows(neighbors) }
          )
        );

        // (4) PERSIST — apply the decided op to the disk.
        match (decision.op) {
          MemoryOp.ADD => {
            let id = baml.id.new();
            self.disk.upsert(id, vec, Mem0.text_of(decision, cand), Mem0.scope_filter(scope));
          },
          MemoryOp.UPDATE => {
            // id preserved, text changes — re-embed the NEW text.
            let id = Mem0.id_of(decision);
            let nv = self.disk.embed(Mem0.text_of(decision, cand));
            self.disk.upsert(id, nv, Mem0.text_of(decision, cand), Mem0.scope_filter(scope));
          },
          MemoryOp.DELETE => { self.disk.delete(Mem0.id_of(decision)); },
          MemoryOp.NONE   => { },   // already covered — the dedup win
        }
        applied.push(decision);
      }
      applied
    } catch (e) {
      // foreign extractor/decider CallErrors and disk Io alike normalize here; a
      // deliberately-thrown MemoryError would pass through with_message unchanged.
      _ => throw baml.UnknownError.with_message<baml.errors.MemoryError>(e, "mem0 add pipeline failed");
    }

    function put(self, scope: Scope, key: string, value: string) -> null
        throws baml.ExtendUnknownError<baml.errors.MemoryError> {
      let v = self.disk.embed(value);
      self.disk.upsert(key, v, value, Mem0.scope_filter(scope))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.MemoryError>(e, "mem0 put failed");
    }

    function get_all(self, scope: Scope) -> Memory[]
        throws baml.ExtendUnknownError<baml.errors.MemoryError> {
      // mem0 has no "list all" on the hot path; emulate with an empty-query scan.
      self.search("", scope, 1000)
    }
  }

  // -- small pure helpers (no self => statics) --
  function render_turns(turns: Turn[]) -> string {
    let s = "";
    for (let t in turns) { s = s + t.role + ": " + t.content + "\n"; }
    s
  }
  function render_rows(rows: VectorRow[]) -> string {
    let s = "";
    for (let r in rows) { s = s + "- [" + r.id + "] " + r.text + "\n"; }
    s
  }
  function text_of(d: MemoryDecision, fallback: string) -> string {
    match (d.text) { let t: string => t, _ => fallback }
  }
  function id_of(d: MemoryDecision) -> string {
    match (d.id) { let i: string => i, _ => baml.sys.panic("UPDATE/DELETE decision missing id") }
  }
}


// -----------------------------------------------------------------------------
// 2b. RawStore — LangGraph BaseStore / ADK State: NO curation. put/search are
//     scoped KV + embedding search, and the policy is entirely the caller's.
//     Proves the MemoryStore interface spans both the curated and raw ends.
// -----------------------------------------------------------------------------
class RawStore {
  disk: VectorStore

  implements MemoryStore {
    function search(self, query: string, scope: Scope, top_k: int) -> Memory[]
        throws baml.ExtendUnknownError<baml.errors.MemoryError> {
      let v = self.disk.embed(query);
      let rows = self.disk.query(v, top_k, Mem0.scope_filter(scope));
      let out: Memory[] = [];
      for (let r in rows) {
        out.push(Memory {
          id: r.id, memory: r.text, metadata: r.metadata, score: r.score,
          created_at: "", updated_at: "",
        });
      }
      out
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.MemoryError>(e, "raw store search failed");
    }

    // raw upsert — NO extract, NO decide. This is the "you reinvent dedup" path.
    function add(self, turns: Turn[], scope: Scope) -> MemoryDecision[]
        throws baml.ExtendUnknownError<baml.errors.MemoryError> {
      let applied: MemoryDecision[] = [];
      for (let t in turns) {
        let id = baml.id.new();
        let v = self.disk.embed(t.content);
        self.disk.upsert(id, v, t.content, Mem0.scope_filter(scope));
        applied.push(MemoryDecision { op: MemoryOp.ADD, id: id, text: t.content });
      }
      applied
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.MemoryError>(e, "raw store add failed");
    }

    function put(self, scope: Scope, key: string, value: string) -> null
        throws baml.ExtendUnknownError<baml.errors.MemoryError> {
      let v = self.disk.embed(value);
      self.disk.upsert(key, v, value, Mem0.scope_filter(scope))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.MemoryError>(e, "raw store put failed");
    }
    function get_all(self, scope: Scope) -> Memory[]
        throws baml.ExtendUnknownError<baml.errors.MemoryError> {
      self.search("", scope, 1000)
    }
  }
}


// -----------------------------------------------------------------------------
// 3. POSTURE A — memory as MIDDLEWARE around the call.
//    A combinator that wraps a Provider: retrieve-before (inject relevant
//    memories into the prompt) and store-after (extract from the exchange).
//    This is the Vercel "system + onFinish" shape. It is structurally identical
//    to the spine's Cache combinator: a plain class that forwards the
//    request/response capability (HttpProvider) and delegates to `inner`. Under
//    the marker spine (../provider-as-marker.md) `call`/`call_with` live on
//    HttpProvider, not on the bare Provider marker, so a combinator that does
//    retrieve-before / store-after around a single call `implements HttpProvider`
//    and forwards both `call` and `call_with` (the latter so the model's
//    usage/sidecar is not dropped — the P1 fix).
//
//    NOTE the awkwardness it exposes (see evaluation): `call<T>(prompt)` gives us
//    only an opaque PromptAst, so we need a host primitive to splice retrieved
//    facts into it, and we have no typed view of the *user's last message* to use
//    as the search query. Both are net-new.
// -----------------------------------------------------------------------------
class WithMemory {
  inner: Provider
  store: MemoryStore
  scope: Scope
  top_k: int

  implements HttpProvider {
    // The member we forward to is itself a Provider; only an HttpProvider member
    // actually has call/call_with, so we route to it by runtime match (the §3
    // combinator pattern). We never build/send/parse ourselves, so the codec
    // methods (build_request/send/parse/meta_of) are inherited unchanged and the
    // Body type is opaque — this combinator only forwards.
    type Body = unknown

    // call_with is the PRIMITIVE here: retrieve-before, forward the projection to
    // the inner HttpProvider (so the model's usage/sidecar survives — the P1 fix),
    // then store-after. `call` derives from it below.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // retrieve-before: derive a search query from the prompt, recall, inject.
      let q = baml.llm.last_user_text(prompt);     // NET-NEW host primitive
      let hits = self.store.search(q, self.scope, self.top_k)
        catch (e) {
          // a recall failure (MemoryError or its UnknownError) is folded onto the
          // call path — the combinator forwards the store's channel into CallError.
          _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "memory search failed");
        };

      let facts = WithMemory.render_facts(hits);
      // splice the recalled facts into the system position of the opaque AST.
      let augmented = baml.llm.with_system_suffix(prompt, facts);   // NET-NEW host primitive

      // forward call_with to the inner request/response provider so `project`
      // reads the inner model's ResponseMeta (usage/logprobs) — not dropped.
      let result: (T, U) = WithMemory.forward_call_with<T, U>(self.inner, augmented, project);
      let answer: T = result.0;

      // store-after: hand the exchange to the layer; it curates implicitly.
      self.store.add(
        [ Turn { role: "user", content: q },
          Turn { role: "assistant", content: baml.llm.text_of<T>(answer) } ],  // NET-NEW: T -> text
        self.scope
      ) catch (e) { _ => { /* memory write is best-effort; don't fail the turn */ } };

      result
    }

    // `call` derives from call_with (project nothing, drop the sidecar) — same
    // derivation the HttpProvider default uses, kept explicit so the retrieve/store
    // wrapping is visible at the call site.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }

  // route call_with to the first request/response member (here, the single inner
  // provider). Errors if the inner provider is not an HttpProvider — the runtime
  // promise a forwarding combinator makes (§3 / §6).
  function forward_call_with<T, U>(inner: Provider, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    match (inner) {
      let h: HttpProvider => h.call_with<T, U>(prompt, project),
      _ => throw baml.errors.Unsupported { message: "memory middleware: inner provider has no request/response call" },
    }
  }

  function render_facts(hits: Memory[]) -> string {
    let s = "What you remember about this user:\n";
    for (let h in hits) { s = s + "- " + h.memory + "\n"; }
    s
  }
}

// Fluent sugar, mirroring Provider.with_retry / .cached in the spine. We add it
// as a free function rather than touch the canonical Provider interface (which
// we are told to build ON, not redefine).
function with_memory(p: Provider, store: MemoryStore, scope: Scope) -> WithMemory {
  WithMemory { inner: p, store: store, scope: scope, top_k: 5 }
}


// -----------------------------------------------------------------------------
// 4. POSTURE B — memory as a TOOL the model drives.
//    Two flavors:
//
//    4a. search_memory as a single Tool. The model decides *when* to recall.
//        This is just an entry in the Tools loop's `tools` array; the dispatch
//        runs store.search. No new capability — it rides the canonical Tools.
//
//    4b. Anthropic memory-tool: the MODEL drives the whole filesystem
//        (view/create/str_replace/insert/delete/rename) against /memories.
//        There is no extract->decide pipeline — the model's judgment is the
//        policy. We model this as a single tool whose `command` field
//        multiplexes the filesystem ops, dispatched against a MemoryStore-backed
//        FS. The Tools capability (begin/step/submit) is reused UNCHANGED; only
//        the dispatch handler is new.
// -----------------------------------------------------------------------------

// 4a — recall as one tool. A helper that builds the Tool descriptor + a handler
// closure the ExecutionContext can dispatch.
class MemoryTools {
  store: MemoryStore
  scope: Scope

  // The Tool descriptor handed into Tools.run_tools(prompt, tools, ctx).
  function search_tool(self) -> Tool {
    Tool {
      name: "search_memory",
      description: "Recall facts you remember about this user, by relevance.",
      parameters: type { query: string, top_k: int? },
    }
  }

  // dispatch body for the search tool — invoked by ExecutionContext.dispatch when
  // the model emits a `search_memory` ToolCall. Returns a ToolResult.
  function handle(self, call: ToolCall) -> ToolResult
      throws baml.ExtendUnknownError<baml.errors.ToolError> {
    let q = arg_str(call.args, "query");
    let hits = self.store.search(q, self.scope, 5);
    ToolResult { id: call.id, output: WithMemory.render_facts(hits) }
  } catch (e) {
    // the store's MemoryError surfaces mid-tool-loop; normalize onto ToolError.
    _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "search_memory dispatch failed");
  }
}

// 4b — Anthropic memory-tool. ONE tool whose `command` selects the FS op. The
// storage backend is ours (the MemoryStore); Anthropic defines the interface and
// the model drives it. `path` is the /memories file; we map path -> store key.
class AnthropicMemoryTool {
  store: MemoryStore
  scope: Scope

  function descriptor(self) -> Tool {
    // The wire type is provider-native (memory_20250818); the provider's
    // build_request emits the special form. Here we expose the logical params
    // the model fills, matching the SDK helper scaffold (BetaAbstractMemoryTool).
    Tool {
      name: "memory",
      description: "Read/write your /memories directory. Read it before each task; write progress as you go.",
      parameters: type {
        command: string,            // view | create | str_replace | insert | delete | rename
        path: string,
        text: string?,
        old_str: string?,
        new_str: string?,
        new_path: string?,
      },
    }
  }

  // The model curates; WE execute. No extract->decide — the model's command IS
  // the decision. This is the structural opposite of Mem0.add().
  function handle(self, call: ToolCall) -> ToolResult
      throws baml.ExtendUnknownError<baml.errors.ToolError> {
    let cmd = arg_str(call.args, "command");
    let path = arg_str(call.args, "path");
    match (cmd) {
      "view" => {
        let rows = self.store.get_all(AnthropicMemoryTool.path_scope(self.scope, path));
        ToolResult { id: call.id, output: WithMemory.render_facts(rows) }
      },
      "create" => {
        self.store.put(self.scope, path, arg_str(call.args, "text"));
        ToolResult { id: call.id, output: "created " + path }
      },
      "str_replace" => {
        // read-modify-write through the store; the model supplied old/new.
        let cur = self.store.get_all(AnthropicMemoryTool.path_scope(self.scope, path));
        let next = WithMemory.render_facts(cur).replace(
          arg_str(call.args, "old_str"),
          arg_str(call.args, "new_str"));
        self.store.put(self.scope, path, next);
        ToolResult { id: call.id, output: "edited " + path }
      },
      "delete" => {
        self.store.put(self.scope, path, "");   // tombstone
        ToolResult { id: call.id, output: "deleted " + path }
      },
      // a command the protocol doesn't define is a capability-negotiation miss;
      // Unsupported is legal on any channel (it implements every capability error).
      _ => { throw baml.errors.Unsupported { message: "unsupported memory command: " + cmd }; }
    }
  } catch (e) {
    // foreign disk Io from the store ops normalizes onto ToolError; the deliberate
    // Unsupported above passes through with_message unchanged.
    _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "memory-tool dispatch failed");
  }

  function path_scope(scope: Scope, path: string) -> Scope {
    // a path under /memories narrows the scope by app_id facet.
    Scope { user_id: scope.user_id, agent_id: scope.agent_id, run_id: scope.run_id, app_id: path }
  }
}


// -----------------------------------------------------------------------------
// 5. WORKING / CORE memory — Letta core blocks: a small mutable block ALWAYS in
//    context, and SELF-EDITING tools the agent uses to rewrite it under pressure.
//    This is the third independent thing (not Session, not Memory). It is *not*
//    relevance-searched — it is always injected, and edited in place.
//
//    The self-editing tools (core_memory_append / core_memory_replace) are
//    Tools, dispatched against the block. So Letta's "self-editing loop" is just
//    the canonical Tools loop whose handlers mutate a CoreMemory.
// -----------------------------------------------------------------------------
class CoreMemory {
  blocks: map<string, string>       // label -> value, e.g. {"human": "...", "persona": "..."}
  limit: int                         // size budget; "memory pressure" when exceeded

  function render(self) -> string {
    let s = "";
    for (let label in self.blocks.keys()) {
      // keys() guarantees presence; narrow the V? get returns to a string.
      let body = match (self.blocks.get(label)) { let v: string => v, _ => "" };
      s = s + "<" + label + ">\n" + body + "\n</" + label + ">\n";
    }
    s
  }

  // self-editing tool handlers — the agent calls these on itself.
  function append_tool() -> Tool {
    Tool { name: "core_memory_append",
           description: "Append a line to one of your always-in-context memory blocks.",
           parameters: type { label: string, content: string } }
  }
  function replace_tool() -> Tool {
    Tool { name: "core_memory_replace",
           description: "Rewrite one of your memory blocks more tersely (use under memory pressure).",
           parameters: type { label: string, value: string } }
  }

  function handle(self, call: ToolCall) -> ToolResult
      throws baml.ExtendUnknownError<baml.errors.ToolError> {
    match (call.name) {
      "core_memory_append" => {
        let label = arg_str(call.args, "label");
        let cur = match (self.blocks.get(label)) { let v: string => v, _ => "" };
        self.blocks.set(label, cur + "\n" + arg_str(call.args, "content"));
        ToolResult { id: call.id, output: "ok" }
      },
      "core_memory_replace" => {
        let label = arg_str(call.args, "label");
        self.blocks.set(label, arg_str(call.args, "value"));
        ToolResult { id: call.id, output: "ok" }
      },
      // a call routed here that isn't a core-memory tool is a negotiation miss.
      _ => { throw baml.errors.Unsupported { message: "not a core-memory tool: " + call.name }; }
    }
  }
}


// -----------------------------------------------------------------------------
// 5b. Letta-as-a-remote-provider. The one honest fit for a SERVER-AUTHORITATIVE
//     stateful agent: a plain Provider whose .call proxies to the Letta server
//     by agent_id. All three memory tiers (core/recall/archival) live server-side
//     and are INVISIBLE to BAML — we cannot model them as BAML MemoryStores
//     because BAML never sees the state. This is the proposal's hard edge: a
//     remote agent is still a request/response provider (HttpProvider), but the
//     typed memory model stops at the wire. (See evaluation, section c.)
// -----------------------------------------------------------------------------
class LettaProxy {
  base_url: string
  agent_id: string       // the server-side stateful object; resumes with full memory

  // LettaProxy genuinely does request/response (POST one turn, get one answer), so
  // it owns call/call_with on HttpProvider — NOT a degenerate/faked call. Its codec
  // is opaque (the host POST is one shot), so call_with is the primitive here and
  // `call` derives from it.
  implements HttpProvider {
    type Body = unknown

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function   // POST {base_url}/agents/{agent_id}/messages ; server mutates its own
                          // memory, returns the answer + a ResponseMeta the host hands to `project`.
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "letta proxy call failed");
    }

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }
}


// -----------------------------------------------------------------------------
// 6. NET-NEW host primitives this scenario forces (collected; flagged in eval).
//    These are the genuine boundaries the middleware posture needs because
//    `call<T>(prompt)` hides the message array behind an opaque PromptAst.
//
//      baml.llm.last_user_text(prompt: PromptAst) -> string
//      baml.llm.with_system_suffix(prompt: PromptAst, text: string) -> PromptAst
//      baml.llm.render(template: string, vars: map<string, unknown>) -> PromptAst
//      baml.llm.text_of<T>(value: T) -> string
//
//    (The map/list/string/id manipulation above is NOT net-new — it is the real
//    stdlib: `Map.get/set/keys`, `Array.push`, `String.replace`, and `baml.id.new()`.
//    Only the four `baml.llm.*` prompt accessors are genuine host boundaries.)
//
//    Declared as externs so the file type-checks; bodies live in the host.
// -----------------------------------------------------------------------------
// (declarations elided — assumed in baml.* host namespaces, like baml.http.send)
