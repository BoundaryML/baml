// The application-facing continuation handle for multi-turn Agent work.
//
// An LLM function promises one thing: call it, get a typed T. An Agent run
// keeps that promise per turn, but the state that makes the NEXT turn exact —
// wire IDs, thinking signatures, server-side response state — is an opaque
// `ai.Conversation` owned by the provider, while the contract that types the
// next turn — output type, tool roster, task identity — lives in the
// application's `ai.Task<T>`. Nothing in the language tied those two halves
// together: applications carried them in separate variables, re-paired them
// by discipline, and expressed "resume" versus "new turn" implicitly by
// whether they remembered to append a message before running.
//
// An AgentSession is that pairing as a value. It exists so that:
//   - the task/conversation marriage is checked at construction, not
//     discovered at runtime by a fingerprint mismatch three turns later;
//   - continuation intent is a named method — `send` starts a new turn,
//     `resume` continues an unfinished one, `submit_tool_results` answers a handoff —
//     instead of an implicit property of what the caller mutated;
//   - saving and restoring across processes has a unit that names its task,
//     because a provider `ConversationToken` alone cannot verify what code
//     it belongs to.
//
// A session advances in place: `send`, `resume`, and `submit_tool_results` run the loop
// and update this session's conversation to the committed result before
// returning the outcome. Hold one session per line of conversation and keep
// using it. To explore two continuations from the same point, `fork()`
// first — it deep-copies the conversation so each branch owns an
// independent copy of the exact shared prefix.
//
// Providers are unaffected: they still implement begin/step/submit (plus
// optional append and save/restore capabilities) and never own the loop.

/// A durable, JSON-serializable reference to a saved session.
///
/// The conversation half is the provider's own `ConversationToken`. The other
/// fields bind that token to the task that produced it, so `restore` can
/// refuse a mismatched pairing instead of letting a wrong-task continuation
/// fail obscurely later. The task itself is code — its tools are function
/// values — so it cannot be serialized; the application re-supplies it at
/// restore time and this token verifies the marriage.
class AgentSessionToken {
    conversation: ai.ConversationToken,
    task_identity: string?,
    output_fingerprint: string?,
    // The structural contract (identity, output type, provider protocol,
    // tool schemas) the session was saved under. Restore refuses a task
    // whose contract differs: "same task" means structurally compatible.
    contract_fingerprint: string?,
}

/// A continuation was started while another continuation is still running
/// on the same session. Sessions are shared mutable objects; concurrent
/// continuations have no defined meaning — run them on forks instead.
class SessionBusy {
    detail: string,
}

implements ai.Failure for SessionBusy {
    // This call did nothing; the OTHER in-flight continuation owns any
    // effects.
    function effects(self) -> ai.Effects throws never { ai.Effects.None }
}

/// A session token or outcome was paired with a task it does not belong to.
class SessionMismatch {
    expected: string?,
    found: string?,
    detail: string,
}

implements ai.Failure for SessionMismatch {
    function effects(self) -> ai.Effects throws never { ai.Effects.None }
}

/// What the model is waiting for, derived from the conversation — never
/// stored, so it cannot drift. `Unreported` means the provider does not
/// expose pending-call state; the runtime guards are skipped for it.
class AwaitingMessage {}

class AwaitingToolResults {
    calls: ai.tools.ToolCall[],
}

class Unreported {}

type SessionPhase = AwaitingMessage | AwaitingToolResults | Unreported

/// One committed continuation of a session — the unit a conversational app
/// renders. Derived from the conversation's portable history plus the
/// per-turn accounting the session records at each commit; there is no
/// second message log.
class Turn {
    /// The portable messages committed during this turn, in order: the
    /// user/steering messages that entered, and whatever assistant/tool
    /// records the provider projects into `messages()`.
    messages: ai.Message[],
    /// This turn's usage (the run's cumulative usage IS the turn delta —
    /// one run per turn).
    usage: ai.Usage,
    /// The final provider step's metadata for the turn, when it finished.
    metadata: ai.ResponseMetadata?,

    /// Concatenated text of the assistant-role messages in this turn.
    function assistant_text(self) -> string throws never {
        self.messages
            .filter((message: ai.Message) -> bool {
                message.role() == ai.MessageRole.Assistant
            })
            .map((message: ai.Message) -> string {
                ai.message_text(message)
            })
            .join("")
    }
}

// Internal per-turn accounting: the portable-history offset where the turn
// began, plus the run's usage and final metadata. Indices, not copies.
class _TurnRecord {
    start: int,
    usage: ai.Usage,
    metadata: ai.ResponseMetadata?,
}


class AgentSession<T> {
    // Underscore-named on purpose: the construction-time invariants only
    // hold when sessions are built through `of`/`from`/`restore`. Writing a
    // literal with these fields bypasses those checks and should read as
    // deliberate internal access. Use `task()`/`conversation()` to read.
    _task: ai.Task<T>,
    _conversation: ai.Conversation,
    // Single-flight: true while a continuation method is running.
    _busy: bool,
    // Per-turn accounting for turns()/last_turn() — offsets into the
    // portable history plus usage/metadata, recorded at each committed
    // continuation. Session-local: it covers turns advanced through THIS
    // session object.
    _turn_records: _TurnRecord[],

    /// The typed task half of this session.
    function task(self) -> ai.Task<T> throws never {
        self._task
    }

    /// The provider-owned exact conversation half of this session.
    function conversation(self) -> ai.Conversation throws never {
        self._conversation
    }

    /// Claims the session for one continuation; throws SessionBusy if
    /// another continuation is already in flight.
    function _acquire(self) -> null throws SessionBusy {
        if (self._busy) {
            throw SessionBusy {
                detail: "another continuation is already running on this session",
            };
        }
        self._busy = true;
        null
    }

    /// Opens a session at turn zero. `provider.begin` renders the task's
    /// prompt into a fresh conversation — no model request happens. Turn one
    /// and turn N are then the same code path: `start(task)`, then
    /// `session.send(...)` for every user turn.
    ///
    /// For conversational use, the task's prompt IS the instructions:
    /// inject documents or context through the task's arguments at this
    /// first render, and send everything dynamic afterwards as messages.
    function start(
        task: ai.Task<T>,
    ) -> AgentSession<T> throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        let provider = match (task.provider) {
            let capability: ai.AgentProvider => capability,
            _ => throw baml.errors.Unsupported {
                message: "provider cannot run an agent: " + task.provider.name(),
            },
        };
        let conversation = provider.begin<T>(task);
        let _ = ai.internal._require_conversation_invariants<T>(conversation, task.provider);
        AgentSession<T> { _task: task, _conversation: conversation, _busy: false, _turn_records: [] }
    }

    /// Pairs a task with the conversation carried by any outcome of running
    /// it. Every outcome variant is accepted — a completed turn, a policy
    /// stop, a handoff, and an interruption are all valid session states;
    /// which continuation methods make sense next depends on which it was.
    /// Sugar over `from` for the common case of holding an outcome.
    function of(
        task: ai.Task<T>,
        outcome: ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed,
    ) -> AgentSession<T> throws ai.Failure {
        let conversation = ai.internal._outcome_conversation<T>(outcome);
        let _ = ai.internal._require_conversation_invariants<T>(conversation, conversation.provider());
        AgentSession<T> { _task: task, _conversation: conversation, _busy: false, _turn_records: [] }
    }

    /// The one constructor for continuation state from either pole:
    ///
    /// - an exact provider-owned `Conversation` (a provider-level restore,
    ///   an imported conversation, or hand-driven begin/step/submit work) —
    ///   validated and used as-is;
    /// - a portable `Messages` history — reconstructed through the task
    ///   provider's `ConversationImportProvider` capability, which may be
    ///   lossy (provider-private reasoning blocks and continuation IDs do
    ///   not survive the portable pole; use the provider-level
    ///   `import_messages` directly when the fidelity report matters).
    ///   Throws `Unsupported` if the provider cannot import.
    function from(
        task: ai.Task<T>,
        source: ai.Conversation | ai.Messages,
    ) -> AgentSession<T> throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        match (source) {
            let conversation: ai.Conversation => {
                //# Refuse a conversation whose declared output type is not T
                let _ = ai.internal._require_conversation_invariants<T>(conversation, conversation.provider());
                AgentSession<T> { _task: task, _conversation: conversation, _busy: false, _turn_records: [] }
            },
            let messages: ai.Messages => {
                let importer = match (task.provider) {
                    let capability: ai.ConversationImportProvider => capability,
                    _ => throw baml.errors.Unsupported {
                        message: "provider cannot import a conversation: " + task.provider.name(),
                    },
                };
                AgentSession<T>.from(task, importer.import_messages<T>(messages).conversation)
            },
        }
    }

    /// One more turn, completed: appends `message`, runs the loop, advances
    /// this session, and returns the final typed value. `send` reports
    /// whatever happened; `complete` demands the finished value.
    ///
    /// Throws `ai.IncompleteRun` if the turn stops early. The session still
    /// advances to that committed checkpoint, so the same session can
    /// `resume` or `submit_tool_results` from a catch site — a demanded
    /// completion never destroys the partial run.
    function complete(
        self,
        message: string | ai.Message | ai.Message[],
        runner: Agent<T> = Agent<T>.new(),
    ) -> T throws ai.IncompleteRun | ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        ai.internal._done<T>(self.send(message, runner = runner)).value
    }

    /// What the model is waiting for right now. Derived from the
    /// conversation on every call: `AwaitingToolResults` carries the pending
    /// calls, so the application does not need to retain the Handoff
    /// outcome to answer it.
    function phase(self) -> SessionPhase throws never {
        match (self._conversation.pending_calls()) {
            null => Unreported {},
            let calls: ai.tools.ToolCall[] => {
                if (calls.length() == 0) {
                    AwaitingMessage {}
                } else {
                    AwaitingToolResults { calls: calls }
                }
            },
        }
    }

    /// The portable projection of this conversation. Crossing to the
    /// portable pole is always this visible call — exact provider state
    /// (thinking signatures, server-side response state) does not survive
    /// it. The dual of the `Messages` arm of `from`.
    function export(self) -> ai.Messages throws never {
        self._conversation.messages()
    }

    /// Injects a mid-conversation instruction that is NOT from the end
    /// user — a compaction summary, fresh context, a policy change. Local:
    /// no model request happens; the note rides into the next `send` or
    /// `resume`. Adapters map it honestly per wire (OpenAI: developer
    /// item; Anthropic/Gemini: tagged user-role content).
    function steer(
        self,
        note: string,
    ) -> null throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        self._conversation.append_message(ai.ChatMessage.steer(note))
    }

    /// The turn-structured transcript of this session: one `Turn` per
    /// committed continuation, derived from the conversation's portable
    /// history and the per-turn accounting recorded at each commit.
    /// Session-local — it covers turns advanced through this session
    /// object (a restored session starts with an empty transcript even
    /// though its conversation carries the full history).
    function turns(self) -> Turn[] throws never {
        let all = self._conversation.messages().items();
        let result: Turn[] = [];
        let i = 0;
        while (i < self._turn_records.length()) {
            let record = self._turn_records[i];
            let end = if (i + 1 < self._turn_records.length()) {
                self._turn_records[i + 1].start
            } else {
                all.length()
            };
            let start = record.start.clamp(0, all.length());
            result.push(Turn {
                messages: all.slice(start, end.clamp(start, all.length())),
                usage: record.usage,
                metadata: record.metadata,
            });
            i = i + 1;
        }
        result
    }

    /// The most recent committed turn, or null before the first one.
    function last_turn(self) -> Turn? throws never {
        let all = self.turns();
        if (all.length() == 0) {
            null
        } else {
            all[all.length() - 1]
        }
    }

    // Record one committed continuation's accounting for turns().
    function _record_turn(
        self,
        turn_start: int,
        outcome: ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed,
    ) -> null throws never {
        let usage = match (outcome) {
            let done: ai.Done<T> => done.usage,
            let stopped: ai.Stopped => stopped.usage,
            let handoff: ai.Handoff => handoff.usage,
            let interrupted: ai.Interrupted => interrupted.usage,
            let failed: ai.Failed => failed.usage,
        };
        let metadata = match (outcome) {
            let done: ai.Done<T> => done.metadata,
            _ => null,
        };
        self._turn_records.push(_TurnRecord {
            start: turn_start,
            usage: usage,
            metadata: metadata,
        });
        null
    }

    /// Moves this session's history to another provider in one call:
    /// export → import → new session on `provider`. Non-destructive — this
    /// session is unchanged and stays valid on its original provider, so a
    /// refused or failed move loses nothing (rollback is free).
    ///
    /// The crossing to the portable pole still happens: provider-private
    /// continuation state (encrypted reasoning blocks, response IDs, cache
    /// handles) does not survive the move. Use the destination's
    /// `import_messages` directly when the fidelity report matters.
    ///
    /// Refuses a session awaiting tool results — the destination provider
    /// never issued those call IDs; answer them with `submit_tool_results`
    /// first.
    function move_to(
        self,
        provider: ai.Provider,
    ) -> AgentSession<T> throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        match (self.phase()) {
            let pending: AwaitingToolResults => {
                throw ai.InvalidRequest {
                    provider: provider.name(),
                    status_code: null,
                    detail: "cannot move a session with unanswered tool calls — call submit_tool_results first",
                };
            },
            _ => {},
        }
        AgentSession<T>.from(self._task.with_provider(provider), self.export())
    }

    /// An independent session sharing this one's exact conversation prefix.
    /// Continuation methods advance provider conversation state in place, so
    /// exploring two continuations from the same point requires forking
    /// first: each fork owns an independent copy and neither branch can
    /// observe the other.
    ///
    /// The copy goes through the provider's own save/restore capability — a
    /// provider-blessed duplicate of opaque continuation state, never a raw
    /// memory copy. Providers without `ResumableAgentProvider` cannot fork.
    function fork(self) -> AgentSession<T> throws baml.errors.UnknownError | baml.errors.Unsupported {
        let resumable = match (self._conversation.provider()) {
            let capability: ai.ResumableAgentProvider => capability,
            _ => throw baml.errors.Unsupported {
                message: "provider cannot fork a session: " + self._conversation.provider().name(),
            },
        };
        let copied = resumable.restore_conversation(resumable.save_conversation(self._conversation));
        AgentSession<T> { _task: self._task, _conversation: copied, _busy: false, _turn_records: self._turn_records.slice(0, self._turn_records.length()) }
    }

    /// Begins a NEW application turn: appends `message` to the exact
    /// provider continuation, runs the agent loop, and advances this session
    /// to the committed result. The task's prompt is not re-rendered — the
    /// conversation already contains it. Requires the conversation to be at
    /// a completed-turn boundary; a conversation with unanswered tool calls
    /// needs `submit_tool_results` instead.
    function send(
        self,
        message: string | ai.Message | ai.Message[],
        runner: Agent<T> = Agent<T>.new(),
    ) -> ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed
        throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        let _ = self._acquire();
        let outcome = self._send_locked(message, runner) catch (e) {
            let unsupported: baml.errors.Unsupported => {
                self._busy = false;
                throw unsupported;
            },
            let failure: ai.Failure => {
                self._busy = false;
                throw failure;
            },
            let unknown: baml.errors.UnknownError => {
                self._busy = false;
                throw unknown;
            },
        };
        self._busy = false;
        outcome
    }

    function _send_locked(
        self,
        message: string | ai.Message | ai.Message[],
        runner: Agent<T>,
    ) -> ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed
        throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        //# String-first: a plain string is the everyday call and desugars
        //# to one user message. A drained queue can be one turn: normalize
        //# to a batch.
        let batch: ai.Message[] = match (message) {
            let text: string => [ai.ChatMessage.user(text)],
            let many: ai.Message[] => many,
            let one: ai.Message => [one],
        };
        //# Intent: new turn. The append happens INSIDE the run boundary —
        //# it is the continuation's first commit (D11).
        let turn_start = self._conversation.messages().items().length();
        let outcome = runner._run(self._task, ai.internal.NextStart { conversation: self._conversation, messages: batch });
        self._conversation = ai.internal._outcome_conversation<T>(outcome);
        let _ = self._record_turn(turn_start, outcome);
        outcome
    }

    /// Continues the SAME unfinished turn — after a `Stopped` stop or
    /// a cooperative interruption — without adding a message, then advances
    /// this session to the committed result. The model picks up from the
    /// last committed checkpoint.
    function resume(
        self,
        runner: Agent<T> = Agent<T>.new(),
    ) -> ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed
        throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        let _ = self._acquire();
        let outcome = self._resume_locked(runner) catch (e) {
            let unsupported: baml.errors.Unsupported => {
                self._busy = false;
                throw unsupported;
            },
            let failure: ai.Failure => {
                self._busy = false;
                throw failure;
            },
            let unknown: baml.errors.UnknownError => {
                self._busy = false;
                throw unknown;
            },
        };
        self._busy = false;
        outcome
    }

    function _resume_locked(
        self,
        runner: Agent<T>,
    ) -> ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed
        throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        //# Intent: resume — append nothing
        let turn_start = self._conversation.messages().items().length();
        let outcome = runner._run(self._task, ai.internal.ResumeStart { conversation: self._conversation });
        self._conversation = ai.internal._outcome_conversation<T>(outcome);
        let _ = self._record_turn(turn_start, outcome);
        outcome
    }

    /// Completes a handoff: submits the application's correlated results
    /// for the pending tool calls, continues the turn, and advances this
    /// session to the committed result. Providers require every pending
    /// call ID to receive exactly one correlated result.
    ///
    /// This is the only continuation method for a conversation stopped at a
    /// Handoff; `send`/`complete` are for completed turns and `resume` for
    /// policy stops and interruptions.
    function submit_tool_results(
        self,
        results: ai.tools.ToolResult[],
        runner: Agent<T> = Agent<T>.new(),
    ) -> ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed
        throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        let _ = self._acquire();
        let outcome = self._submit_locked(results, runner) catch (e) {
            let unsupported: baml.errors.Unsupported => {
                self._busy = false;
                throw unsupported;
            },
            let failure: ai.Failure => {
                self._busy = false;
                throw failure;
            },
            let unknown: baml.errors.UnknownError => {
                self._busy = false;
                throw unknown;
            },
        };
        self._busy = false;
        outcome
    }

    function _submit_locked(
        self,
        results: ai.tools.ToolResult[],
        runner: Agent<T>,
    ) -> ai.Done<T> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed
        throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        //# Intent: answer. The submit happens INSIDE the run boundary — it
        //# is the continuation's first commit (D11).
        let turn_start = self._conversation.messages().items().length();
        let outcome = runner._run(self._task, ai.internal.AnswerStart { conversation: self._conversation, results: results });
        self._conversation = ai.internal._outcome_conversation<T>(outcome);
        let _ = self._record_turn(turn_start, outcome);
        outcome
    }

    /// Saves this session as a small, JSON-serializable token. The provider
    /// serializes the conversation half (`ResumableAgentProvider`); the task
    /// half is recorded as identity plus output fingerprint for `restore` to
    /// verify. Sessions exist only at committed outcome boundaries, so every
    /// save point is a consistent, resumable state.
    function save(
        self,
    ) -> AgentSessionToken throws baml.errors.UnknownError | baml.errors.Unsupported {
        let resumable = match (self._conversation.provider()) {
            let capability: ai.ResumableAgentProvider => capability,
            _ => throw baml.errors.Unsupported {
                message: "provider cannot save a session: " + self._conversation.provider().name(),
            },
        };
        AgentSessionToken {
            conversation: resumable.save_conversation(self._conversation),
            task_identity: match (self._task.identity) {
                let identity: ai.TaskIdentity => identity.package + "." + identity.name,
                null => null,
            },
            output_fingerprint: self._conversation.output_type_fingerprint(),
            contract_fingerprint: self._task.contract_fingerprint(),
        }
    }

    /// Restores a saved session in a fresh process. The application
    /// re-supplies the task — tasks are code and do not serialize — and the
    /// token verifies it is the SAME task the session was saved under before
    /// any model request happens. The task's provider must implement
    /// `ResumableAgentProvider`; restoring re-establishes ownership on the
    /// new provider instance.
    function restore(
        task: ai.Task<T>,
        token: AgentSessionToken,
    ) -> AgentSession<T> throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
        let expected = match (task.identity) {
            let identity: ai.TaskIdentity => identity.package + "." + identity.name,
            null => null,
        };
        if (token.task_identity != null && expected != token.task_identity) {
            throw SessionMismatch {
                expected: token.task_identity,
                found: expected,
                detail: "session token was saved under a different task",
            };
        }
        match (token.contract_fingerprint) {
            let saved: string if saved != task.contract_fingerprint() => {
                throw SessionMismatch {
                    expected: saved,
                    found: task.contract_fingerprint(),
                    detail: "session token was saved under a structurally different task contract (output type, provider protocol, or tool schemas differ)",
                };
            },
            _ => {},
        }
        let resumable = match (task.provider) {
            let capability: ai.ResumableAgentProvider => capability,
            _ => throw baml.errors.Unsupported {
                message: "provider cannot restore a session: " + task.provider.name(),
            },
        };
        let conversation = resumable.restore_conversation(token.conversation);
        let fingerprint = conversation.output_type_fingerprint();
        if (token.output_fingerprint != null && fingerprint != token.output_fingerprint) {
            throw SessionMismatch {
                expected: token.output_fingerprint,
                found: fingerprint,
                detail: "restored conversation declares a different output type",
            };
        }
        //# The restored conversation must still match T
        let _ = ai.internal._require_conversation_invariants<T>(conversation, conversation.provider());
        AgentSession<T> { _task: task, _conversation: conversation, _busy: false, _turn_records: [] }
    }
}
