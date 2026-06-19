// =============================================================================
// Scenario 17 — Within-run history + sessions
// usage.baml  —  APPLICATION-author code
//
// What the app author writes to hold a conversation. The point: the transcript
// threading and per-provider wire divergence live in implement.baml; here the app
// only picks a provider, names a Session, and calls a `.chat` companion.
// =============================================================================


// -----------------------------------------------------------------------------
// 1. Clients — config only. The system prompt is a provider FIELD, because where
//    it lives on the wire is a per-provider decision (see implement.baml build_chat).
//    The two providers diverge (in-array vs top-level system) but the client blocks
//    read identically — that's the portability claim.
// -----------------------------------------------------------------------------

client Terse() {
  OpenAIChat {
    model: "gpt-4o",
    api_key: env.OPENAI_API_KEY,
    system: "You are a terse assistant.",
  }
}

client TerseClaude() {
  AnthropicChat {
    model: "claude-sonnet-4-5",
    api_key: env.ANTHROPIC_API_KEY,
    max_tokens: 1024,
    system: "You are a terse assistant.",        // SAME field; lands top-level on the wire
  }
}

// A windowed client: wrap a conversational provider, keep the last 8 turns. Reads
// like a combinator chain even though it rewrites history, not delivery.
client TerseWindowed() {
  Windowed { inner: OpenAIChat {
    model: "gpt-4o", api_key: env.OPENAI_API_KEY, system: "You are a terse assistant.",
  }, keep: 8 }
}

// A summarizing client: fold turns older than 6 into a synopsis written by a cheap
// model. The summarizer is just another Provider — composition all the way down.
client TerseSummarizing() {
  Summarizing {
    inner: AnthropicChat {
      model: "claude-sonnet-4-5", api_key: env.ANTHROPIC_API_KEY,
      max_tokens: 1024, system: "You are a terse assistant.",
    },
    summarizer: OpenAIChat { model: "gpt-4o-mini", api_key: env.OPENAI_API_KEY, system: "Summarize." },
    keep: 6,
  }
}


// -----------------------------------------------------------------------------
// 2. The conversational function + its `.chat` companion.
//
// A plain `Ask(text)` would use Provider.call and forget everything. To get
// history threading the function declares it is conversational by taking a Session
// and routing through the `.chat` companion, which `match`es the client's provider
// against the Conversational capability (and ERRORS if it isn't — there's no honest
// single-shot fallback for "remember the conversation").
// -----------------------------------------------------------------------------

function Ask(text: string) -> string {
  client: Terse()
  prompt #"{{ text }}"#
}

// Ask.chat(text, session) — the companion the compiler generates from the `.chat`
// capability. Shown longhand to make the negotiation explicit:
function Ask$chat(text: string, session: Session) -> string {
  let p = Terse();
  match (p) {
    let c: Conversational => c.chat<string>(render_prompt(text), session),
    _ => throw baml.errors.Unsupported { message: "client cannot hold a conversation" },
  }
}


// -----------------------------------------------------------------------------
// 3. Driving a multi-turn conversation — the background's `ask()` loop, but the
//    array append (user AND assistant) is hidden inside `.chat`. The classic
//    "forgot to append the assistant turn" bug is structurally impossible here.
// -----------------------------------------------------------------------------

function Conversation() -> string throws baml.errors.Io {
  let s = Session.ephemeral("demo-1");
  // turn 1 — the array starts empty; .chat records user+assistant.
  let _a = Ask$chat("What's the capital of France?", s);
  // turn 2 — "its" resolves only because turn 1 is in the store.
  let b = Ask$chat("And its population?", s);
  return b;
}


// -----------------------------------------------------------------------------
// 4. Portability proof: the SAME driver over Anthropic. The app code is byte-for-
//    byte identical except the client; the in-array-vs-top-level system divergence
//    is fully absorbed by AnthropicChat.build_chat.
// -----------------------------------------------------------------------------

function ConversationClaude() -> string throws baml.errors.Io {
  let s = Session.ephemeral("demo-2");
  let p = TerseClaude();
  match (p) {
    let c: Conversational => {
      let _a = c.chat<string>(render_prompt("What's the capital of France?"), s);
      return c.chat<string>(render_prompt("And its population?"), s);
    },
    _ => throw baml.errors.Unsupported { message: "not conversational" },
  }
}


// -----------------------------------------------------------------------------
// 5. Unbounded growth handled by the wrapper — the app opts into a policy by
//    swapping the client, nothing else. The Session and the call site are unchanged.
// -----------------------------------------------------------------------------

function LongConversation() -> string throws baml.errors.Io {
  let s = Session.new("support-ticket-4471", MemoryStore.new());
  let p = TerseSummarizing();                     // older turns auto-folded into a synopsis
  match (p) {
    let c: Conversational => {
      let _1 = c.chat<string>(render_prompt("My checkout is failing."), s);
      let _2 = c.chat<string>(render_prompt("I'm on the latest app version."), s);
      let _3 = c.chat<string>(render_prompt("It happens only with PayPal."), s);
      // ... by now history > keep; build_chat folds the oldest turns into a summary.
      return c.chat<string>(render_prompt("So what's the root cause?"), s);
    },
    _ => throw baml.errors.Unsupported { message: "not conversational" },
  }
}
