// ═══════════════════════════════ baml.ai — map ═══════════════════════════════
// Directories are cosmetic (same namespace everywhere); they exist so you can see
// at a glance where things belong:
//
//   core/           the spine: messages (this file), the Provider marker, ResponseMeta
//   capabilities/   one self-contained file per capability interface —
//                   http, streaming, constrained, tools, conversation, chain,
//                   background, memory, cache, suspend, realtime, introspection
//   combinators.baml   Fallback / Retry / RoundRobin (plain classes over Provider)
//   providers/      concrete implementations: openai, openai_responses, openai_strict,
//                   openai_realtime, anthropic, gemini, harness
//
// A `Provider` is a bare marker; each capability is an interface that
// `requires Provider` and owns its own interaction method. Built-in providers are
// ordinary classes that `implements` these — the same way a user's custom provider
// would. Convention: `_`-prefixed members are private and sit at the bottom of their
// declaration; `*Wire` classes are a provider's private wire-envelope shapes.
//
// ── Prompts: string vs PromptAst ──
// The provider exchange type is the NATIVE `ChatMessage[]` (roles + text/media parts);
// `call_messages_with` is THE primitive. The `prompt: string` entry points (`call`,
// `call_with`, `stream`, …) are sugar for a single user message. `PromptAst` never
// reaches a provider: it exists only at the declarative-DSL boundary — an LLM
// `function`'s backtick template renders to a `PromptAst`, and `prompt_to_messages`
// (below) is the ONE bridge into `ChatMessage[]`. Everything downstream is BAML.

/// One part of a chat message: text or one media item. A product type (exactly one field
/// set) rather than a union so it crosses the host boundary simply; use the
/// `MessagePart.from_*` constructors.
class MessagePart {
    text: string?,
    image: image?,
    audio: audio?,
    pdf: pdf?,
    video: video?,

    function from_text(text: string) -> MessagePart throws never {
        MessagePart { text: text }
    }
    function from_image(img: image) -> MessagePart throws never {
        MessagePart { image: img }
    }
    function from_audio(a: audio) -> MessagePart throws never {
        MessagePart { audio: a }
    }
}

/// One chat message: a role plus interleaved text/media parts. The native BAML
/// representation of what goes on the wire.
class ChatMessage {
    role: string,
    parts: MessagePart[],

    function user(text: string) -> ChatMessage throws never {
        ChatMessage { role: "user", parts: [MessagePart.from_text(text)] }
    }
    function system(text: string) -> ChatMessage throws never {
        ChatMessage { role: "system", parts: [MessagePart.from_text(text)] }
    }
    function assistant(text: string) -> ChatMessage throws never {
        ChatMessage { role: "assistant", parts: [MessagePart.from_text(text)] }
    }
}

/// Convert a rendered `root.llm.PromptAst` (a host type) into the native `ChatMessage[]`
/// representation — roles preserved, text and media parts interleaved. This is the ONE
/// leaf bridge between the render pipeline and a BAML-authored provider; everything
/// downstream (request building, wire encoding) is BAML.
function prompt_to_messages(prompt: root.llm.PromptAst) -> ChatMessage[] throws never {
    $rust_io_function
}

/// Convert native `ChatMessage[]` back into a rendered `root.llm.PromptAst` —
/// the reverse of `prompt_to_messages` (roles preserved, text and media parts
/// interleaved). The seam the `LegacyClient` bridge uses to feed
/// already-rendered messages into the legacy request pipeline.
function messages_to_prompt(messages: ChatMessage[]) -> root.llm.PromptAst throws never {
    $rust_io_function
}
