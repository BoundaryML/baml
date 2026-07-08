// Phase D (DCP §1.4): a USER-declared capability + companion driver — the
// open-registry thesis. The `//baml:llm_capability` interface and the
// `//baml:llm_companion(moderated)` driver live entirely in user code; the
// compiler generates a `Foo$moderated` companion on every LLM function, and
// negotiation happens at runtime against whatever client is passed.

//baml:llm_capability
interface Moderated requires baml.ai.Provider {
    function call_moderated<T>(self, messages: baml.ai.ChatMessage[], policy: string) -> T
        throws baml.errors.UnknownError
}

//baml:llm_companion(moderated)
function drive_moderated<T>(client: baml.ai.Provider, prompt: baml.llm.PromptAst, policy: string) -> T
    throws baml.errors.Unsupported | baml.errors.UnknownError {
    let p = client;
    match (p) {
        let m: Moderated => m.call_moderated<T>(baml.ai.prompt_to_messages(prompt), policy),
        _ => throw baml.errors.Unsupported { message: "client's provider is not Moderated" },
    }
}

/// Offline Moderated provider: prefixes the policy tag onto its scripted reply.
class GuardedEcho {
    reply: string,

    implements baml.ai.Provider {}

    implements Moderated {
        function call_moderated<T>(self, messages: baml.ai.ChatMessage[], policy: string) -> T
            throws baml.errors.UnknownError {
            baml.sap.parse<T>("[" + policy + "] " + self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["moderated parse failed"] },
            }
        }
    }
}

client<llm> DeadModerated {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function ComposeNote(q: string) -> string {
  client DeadModerated
  prompt #"Compose a note about: {{ q }}"#
}

test "user capability: generated companion routes through the user's provider" {
    let r = ComposeNote$moderated("turtles", "no-pii", client = GuardedEcho { reply: "a draft note" }) catch (e) {
        _ => "companion failed",
    };
    assert.equal(r, "[no-pii] a draft note")
}

test "user capability: declared client without the capability is a typed Unsupported" {
    let r = ComposeNote$moderated("turtles", "no-pii") catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}

test "user capability: the driver itself is directly callable (the no-sugar path)" {
    let prompt = ComposeNote$render_prompt("turtles");
    let r = drive_moderated<string>(GuardedEcho { reply: "direct" }, prompt, "strict") catch (e) {
        _ => "driver failed",
    };
    assert.equal(r, "[strict] direct")
}
