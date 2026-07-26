// Session creation has no task receiver. Options/token configure a runner whose
// exact input is the provider that will own the resulting Session resource.
class OpenSession {
    options: root.ai.SessionOptions,

    function new(
        session_id: string? = null,
        attributes: map<string, string> = {},
    ) -> OpenSession throws never {
        OpenSession {
            options: root.ai.SessionOptions { session_id: session_id, attributes: attributes },
        }
    }

    implements root.ai.Runner<root.ai.Provider> {
        type Output = root.ai.Session
        type Error = baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            provider: root.ai.Provider,
        ) -> root.ai.Session throws baml.errors.UnknownError | baml.errors.Unsupported {
            match (provider) {
                let sessions: root.ai.SessionProvider => sessions.open_session(self.options),
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot open sessions",
        }
                },
            }
        }
    }
}

class ResumeSession {
    token: root.ai.SessionToken,

    function new(token: root.ai.SessionToken) -> ResumeSession throws never {
        ResumeSession { token: token }
    }

    implements root.ai.Runner<root.ai.Provider> {
        type Output = root.ai.Session
        type Error = baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            provider: root.ai.Provider,
        ) -> root.ai.Session throws baml.errors.UnknownError | baml.errors.Unsupported {
            match (provider) {
                let sessions: root.ai.SessionProvider => sessions.resume_session(self.token),
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot resume sessions",
        }
                },
            }
        }
    }
}

class SaveConversation {
    conversation: root.ai.Conversation,

    function new(conversation: root.ai.Conversation) -> SaveConversation throws never {
        SaveConversation { conversation: conversation }
    }

    implements root.ai.Runner<root.ai.Provider> {
        type Output = root.ai.ConversationToken
        type Error = baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            provider: root.ai.Provider,
        ) -> root.ai.ConversationToken throws baml.errors.UnknownError | baml.errors.Unsupported {
            match (provider) {
                let resumable: root.ai.ResumableToolCallingProvider => {
                    resumable.save_conversation(self.conversation)
                },
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot save conversations",
        }
                },
            }
        }
    }
}

class RestoreConversation {
    token: root.ai.ConversationToken,

    function new(token: root.ai.ConversationToken) -> RestoreConversation throws never {
        RestoreConversation { token: token }
    }

    implements root.ai.Runner<root.ai.Provider> {
        type Output = root.ai.Conversation
        type Error = baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            provider: root.ai.Provider,
        ) -> root.ai.Conversation throws baml.errors.UnknownError | baml.errors.Unsupported {
            match (provider) {
                let resumable: root.ai.ResumableToolCallingProvider => {
                    resumable.restore_conversation(self.token)
                },
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot restore conversations",
        }
                },
            }
        }
    }
}
