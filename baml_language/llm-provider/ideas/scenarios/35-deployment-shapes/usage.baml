// ============================================================================
// Application-author code: the SAME function, four homes.
//
// The payoff to show: the analysis function below is written ONCE. What changes
// across deployments is only the `client` it is given — and a client is just a
// function, so "which home" is a function call, not a config dialect.
// ============================================================================

// --- The four clients (config only; each differs ONLY in where the secret lives) ---

// (1) Long-lived server: key from an env var, plain BearerKey.
client ServerChat() {
  OpenAIChat { model: "gpt-4o", auth: BearerKey { value: env.OPENAI_API_KEY } }
}

// (2) Edge / serverless: secret comes from the platform binding, not .env. Still a
//     BearerKey — the provider body is identical; only the SOURCE expression changed.
client EdgeChat() {
  OpenAIChat { model: "gpt-4o", auth: BearerKey { value: env.PLATFORM_OPENAI_BINDING } }
}

// (3) Cloud-IAM (server or edge-with-SDK): no bearer key at all; the credential
//     chain signs each request. Same OpenAIChat-shaped interface, Bedrock provider.
client IamChat(region: string) {
  BedrockChat {
    model: "anthropic.claude-3-5-sonnet-20241022-v2:0",
    region: region,
    auth: IamCredential { scheme: "aws-sigv4", region: region },
  }
}

// (4) Browser: NO key field is even constructible here. The browser receives a
//     provider built by the server-side factory. This `client` is what the browser
//     code calls AFTER the server handed it a minted OpenAIRealtime.
//
// The factory call (SessionMint.for_browser) runs on the server; its result is
// serialized to the browser, which wraps it back into a client to use the sugar.
client BrowserVoice(minted: OpenAIRealtime) {
  minted          // already carries only the EphemeralToken — safe
}

// (4b) Browser, plain-HTTP (no ephemeral-key support): proxy through same origin.
client BrowserProxiedChat() {
  ProxiedThrough { proxy_path: "/api/llm", model: "gpt-4o" }
}

// ---------------------------------------------------------------------------
// ONE analysis function. Note it names no home, no key, no transport. It takes a
// provider param so the SAME body runs under any of the four clients above.
// ---------------------------------------------------------------------------

class Summary {
  topic string
  bullets string[]
}

function Analyze(text: string, who: Provider) -> Summary {
  client: who
  prompt #"Summarize into a topic and bullets:\n{{ text }}"#
}

// Server home:
//   Analyze("...", ServerChat())
// Edge home (identical call, different client):
//   Analyze("...", EdgeChat())
// IAM home:
//   Analyze("...", IamChat("us-east-1"))
// Browser-proxied home:
//   Analyze("...", BrowserProxiedChat())
//
// All four share ONE catch surface on the CallError channel
// (baml.ExtendUnknownError<baml.errors.CallError>) — the IAM-chain-absent failure
// and a transport blip triage the same way regardless of home:
//   Analyze("...", IamChat("us-east-1")) catch (e) {
//     let c: CredentialUnavailable => "[no cloud-IAM chain in this home: " + c.reason + "]";
//     let h: HttpTransportError    => retry();                          // is_network_error() == true
//     let e2: baml.errors.CallError => { if (e2.is_network_error()) retry(); else fail(); }
//     let u: baml.UnknownError     => report(u.message);               // the escape hatch
//   }
//
// Streaming companion — viable in ALL FOUR because SSE is universal:
//   Analyze.stream("...", EdgeChat())          // edge can stream within its budget
//   Analyze.stream("...", BrowserProxiedChat()) // browser streams from same-origin

// ---------------------------------------------------------------------------
// The realtime (socket) home — the one that does NOT run everywhere.
// ---------------------------------------------------------------------------

function VoiceSession(system: string, who: Provider, io: Channel) -> Transcript {
  client: who
  prompt #"{{ system }}"#
}

// Browser flow, end to end:
//   // 1. SERVER route handler — mints, returns a browser-safe provider:
//   let browserProvider = SessionMint.for_browser(env.OPENAI_API_KEY, "alloy");
//   //    (serialize browserProvider to the client; the BearerKey stayed on the server)
//
//   // 2. BROWSER — wraps it and opens the duplex over a WebRTC channel:
//   VoiceSession.live("be terse", BrowserVoice(browserProvider), io)
//      -> OpenAIRealtime.run(prompt, io)   // host_can_hold("webrtc") == true in a browser
//
// Edge attempt (FAILS LOUDLY, by design):
//   VoiceSession.live("be terse", BrowserVoice(p_with_ws_transport), io)
//      -> throws TransportUnviable { transport: "websocket" }   // a typed RealtimeError
//         because host_can_hold("websocket") == false on a plain serverless function.
//
// The caller catches it most-specific-first on the RealtimeError channel
// (baml.ExtendUnknownError<baml.errors.RealtimeError>):
//   VoiceSession.live("be terse", BrowserVoice(p), io) catch (e) {
//     let t: TransportUnviable      => fallBackToTextChat();          // concrete realtime-domain miss
//     let c: CredentialUnavailable  => remintFromServer();           // stale/absent token
//     let r: baml.errors.RealtimeError => { if (r.is_network_error()) retry(); else fail(); }
//     let u: baml.UnknownError      => report(u.message);            // the escape hatch
//   }

// ---------------------------------------------------------------------------
// Cron / durable home — combinator chains read the same regardless of host.
// A scheduled job: no user waiting, so wrap aggressive orchestration. A durable
// object CAN hold the socket, so the same VoiceSession works there too.
// ---------------------------------------------------------------------------

client CronChat() {
  // generous, retry-heavy: nobody is watching. Combinators wrap the provider;
  // the provider itself is unchanged from the server home.
  ServerChat().with_retry(5).fallback_to(IamChat("us-west-2"))
}

function NightlyDigest(corpus: string) -> Summary {
  client: CronChat()
  prompt #"Digest overnight:\n{{ corpus }}"#
}

// Durable object: identical realtime call to the browser one — but it's the server
// key (BearerKey), and host_can_hold("websocket") is TRUE here, so WS is allowed.
client DurableVoice() {
  OpenAIRealtime { voice: "alloy", auth: BearerKey { value: env.OPENAI_API_KEY }, transport: "websocket" }
}
//   VoiceSession.live("...", DurableVoice(), io)   // the one serverless shape that holds a socket
