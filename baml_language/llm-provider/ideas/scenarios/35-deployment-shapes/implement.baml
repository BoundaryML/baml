// ============================================================================
// Deployment shapes — server / browser / edge / durable
//
// One provider layer, four homes. What changes between homes is NOT the
// provider's wire behavior — it's three orthogonal things:
//
//   (1) WHERE the secret comes from   (env var | platform binding | IAM chain
//                                       | an ephemeral token minted elsewhere)
//   (2) HOW LONG a connection can live (SSE everywhere; WS/WebRTC only where a
//                                       host can hold a socket)
//   (3) WHO holds the long-lived key   (server holds it; browser gets a
//                                       session-scoped token the server minted)
//
// The proposal already gives us: providers are classes, options are fields,
// `client` is a function. That means "where the secret comes from" is just a
// constructor argument, and "the server mints a token the browser uses" is a
// provider FACTORY — an ordinary function that returns a Provider. We build on
// the canonical Provider / HttpProvider / Realtime spine verbatim; everything
// below is net-new surface this scenario needs.
// ============================================================================

// ----------------------------------------------------------------------------
// Net-new host primitives (genuine boundaries — secret resolution + transport
// probing are environment facts the BAML runtime cannot synthesize itself).
// ----------------------------------------------------------------------------

// A credential is NOT always a plain string. It can be a static bearer key, a
// short-lived minted token (with an expiry), or a deferred cloud-IAM credential
// chain that must be resolved per-request. We model it as a capability so a
// provider asks for an Authorization header the same way regardless of home.
interface Credential {
  // Resolve the value to attach. For a static key this is a no-op; for an IAM
  // chain it walks the cloud SDK's credential providers; for a minted token it
  // returns the cached token (and may throw if it has expired and can't refresh).
  // On the call path (build_request) and the realtime path (run) — declare the
  // CallError channel; realtime forwards it through its own RealtimeError channel.
  function authorize(self, request: baml.http.Request) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.CallError>
}

// A credential failure that is NOT transport: the chain is absent (browser/edge
// with no cloud SDK) or the minted token has expired. Distinct from a network
// blip — the app branches on it ("re-mint", "this home can't do IAM").
class CredentialUnavailable {
  reason: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  // The same credential resolves the SSE stream-start and the realtime socket
  // upgrade, so it can surface on those paths too.
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool { return false; }
  }
  implements baml.errors.RealtimeError {
    function is_network_error(self) -> bool { return false; }
  }
}

// A plain bearer key. Works in every home that can SEE the secret (server, edge
// platform binding, CI). NEVER constructed in a browser bundle.
class BearerKey {
  value: string
  implements Credential {
    function authorize(self, request: baml.http.Request) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      request.headers.set("Authorization", "Bearer " + self.value);
      request
    }
  }
}

// A cloud-IAM credential (Bedrock / Vertex). The Authorization is a SigV4 / OAuth
// signature computed over the request from a credential chain that only exists on
// a host with the cloud SDK present. In a browser or a minimal edge runtime, the
// chain resolves to nothing and `authorize` throws — that failure is the honest
// answer to "this home can't do cloud-IAM."
class IamCredential {
  // which chain: "aws-sigv4" | "gcp-oauth"
  scheme: string
  region: string
  implements Credential {
    function authorize(self, request: baml.http.Request) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // $rust_io_function walks the platform credential chain (env, instance
      // metadata, web-identity, ...) and signs `request`. On a host with no
      // chain (browser/edge-without-binding) it throws CredentialUnavailable.
      $rust_io_function
    } catch (e) {
      // No chain present is a classified credential failure; anything else
      // (transient signing I/O) is normalized into the universal wrapper.
      let c: CredentialUnavailable => throw c;
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "iam sigv4/oauth signing failed");
    }
  }
}

// A minted, session-scoped token: the value the BROWSER holds. It carries its own
// expiry so a provider can refuse to start a session it knows will die mid-call.
class EphemeralToken {
  value: string
  expires_at_ms: int        // epoch milliseconds, comparable to baml.sys.now_ms()
  implements Credential {
    function authorize(self, request: baml.http.Request) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      if (baml.sys.now_ms() >= self.expires_at_ms) {
        throw CredentialUnavailable { reason: "ephemeral token expired; re-mint from the server" };
      }
      request.headers.set("Authorization", "Bearer " + self.value);
      request
    }
  }
}

// A classified transport failure: the socket/HTTP send itself failed (DNS, reset,
// timeout). Classified as network so the cron home's retry combinator can triage
// it without knowing the concrete class. Surfaces on the call path and mid-stream.
class HttpTransportError {
  detail: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return true; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool { return true; }
  }
}

// Transport viability is an environment fact. A provider asks the host what it can
// hold BEFORE choosing a transport, instead of discovering it via a dead socket.
// "sse" | "websocket" | "webrtc" -> can this home keep it open?
function host_can_hold(transport: string) -> bool {
  // $rust_io_function: the runtime knows its own home (server | edge | browser |
  // durable-object). Returns true for "sse" everywhere; true for "websocket" only
  // on a socket-holding host (server, durable object); etc.
  $rust_io_function
}

// ----------------------------------------------------------------------------
// The ONE provider that runs unchanged in all four homes.
//
// Its fields are exactly: the model, and a Credential. NOT a string api_key —
// because "where the secret comes from" is the whole point of variation here,
// and a Credential absorbs all of (static key | IAM | minted token) behind one
// `authorize`. The provider body is identical regardless of home.
// ----------------------------------------------------------------------------

class OpenAIChat {
  model: string
  auth: Credential          // the divergence point — not `api_key: string`

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Identical in every home: build the unauthenticated request, then let the
      // Credential attach auth. The provider does not know or care which home.
      // authorize already declares the CallError channel, so its CredentialUnavailable
      // flows straight through.
      let base = baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "Content-Type": "application/json" },
        body: baml.llm.openai_chat_body<T>(self.model, prompt),   // SAP schema goes in the prompt text
      };
      self.auth.authorize(base)
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw HttpTransportError { detail: "openai http send failed" }; }
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
  }

  // Streaming via SSE works in EVERY home (it's a one-shot HTTP response that
  // happens to chunk). This is the transport that is always viable.
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.ExtendUnknownError<baml.errors.StreamError> {
      let base = baml.http.Request {
        method: "POST",
        url: "https://api.openai.com/v1/chat/completions",
        headers: { "Content-Type": "application/json" },
        body: baml.llm.openai_chat_body_stream<TStream>(self.model, prompt),
      };
      // authorize declares the CallError channel; here we are on the StreamError
      // channel. CredentialUnavailable implements both, so it survives as a typed
      // StreamError; only a genuinely-foreign value falls through to UnknownError.
      let signed = self.auth.authorize(base) catch (e) {
        let cu: CredentialUnavailable => throw cu;
        _ => throw baml.UnknownError.with_message<baml.errors.StreamError>(e, "openai stream auth failed");
      };
      let sse = baml.http.fetch_sse(signed) catch (e) { _ => throw HttpTransportError { detail: "openai sse failed" }; };
      baml.llm.stream_from_sse<TStream, TFinal>(sse)
    }
  }
}

// ----------------------------------------------------------------------------
// Bedrock — the cloud-IAM divergence. SAME interface, DIFFERENT auth + URL.
// Its only difference from OpenAIChat is that its Credential is an IamCredential
// and the URL is region-scoped. It deliberately does NOT implement a socket
// transport — Bedrock streaming is HTTP event-stream, fine; WebRTC isn't a thing.
// ----------------------------------------------------------------------------

// Typed view of the Bedrock `converse` response envelope, so `parse` reads the
// model text through a class (baml.json.from_string) instead of a fictional
// Response.json_path. Only the path we consume is modeled.
class BedrockContentBlock { text: string }
class BedrockMessage      { content: BedrockContentBlock[] }
class BedrockOutput       { message: BedrockMessage }
class BedrockConverseResp { output: BedrockOutput }

class BedrockChat {
  model: string
  region: string
  auth: Credential          // expected to be an IamCredential, but typed as the interface

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let base = baml.http.Request {
        method: "POST",
        url: "https://bedrock-runtime." + self.region + ".amazonaws.com/model/" + self.model + "/converse",
        headers: { "Content-Type": "application/json" },
        body: baml.llm.bedrock_converse_body<T>(self.model, prompt),
      };
      self.auth.authorize(base)     // IamCredential signs SigV4; throws CredentialUnavailable if no chain
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw HttpTransportError { detail: "bedrock http send failed" }; }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Decode the envelope into a typed class, navigate to the model text, then
      // SAP-parse that text into T. (Response has no .json_path.)
      let env = baml.json.from_string<BedrockConverseResp>(from.text());
      let block = env.output.message.content.at(0);
      match (block) {
        null => throw baml.UnknownError.with_message<baml.errors.CallError>(
          "empty bedrock content", "bedrock parse failed");
        let b: BedrockContentBlock => baml.sap.parse<T>(b.text),
      }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "bedrock parse failed");
    }
  }
  // No Streaming-over-SSE here on purpose: Bedrock uses a binary event-stream;
  // a faithful impl would add it, but the teaching point is that the capability
  // SET differs per provider even though both are HttpProviders.
}

// ----------------------------------------------------------------------------
// Realtime over a socket — the transport that is NOT viable everywhere.
//
// OpenAI Realtime needs a WebSocket/WebRTC connection that survives the whole
// session. That host requirement is encoded INSIDE the provider: `run` probes
// `host_can_hold` and refuses on a home that can't keep the socket, rather than
// dying mid-session on edge/serverless.
// ----------------------------------------------------------------------------

// The home cannot hold the requested socket transport for a full session. A
// distinct realtime-domain failure, surfaced up-front (not a dropped connection).
// Not network, not a credential problem — a deployment-shape mismatch.
class TransportUnviable {
  transport: string         // "websocket" | "webrtc"
  implements baml.errors.RealtimeError {
    function is_network_error(self) -> bool { return false; }
  }
}

class OpenAIRealtime {
  voice: string
  auth: Credential          // in the browser this is an EphemeralToken; on a server a BearerKey
  transport: string         // "websocket" | "webrtc"

  // Realtime requires Provider (the marker), so OpenAIRealtime IS a Provider with no
  // explicit `implements Provider` and no fabricated `call` — it exposes ONLY its real
  // interaction, `run`. A one-shot over a duplex socket was always a degenerate fake.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      // The home check: refuse on a host that can't hold the socket. This is the
      // "30s realtime session outlives a serverless budget" failure, surfaced
      // up-front instead of as a dropped connection.
      if (!host_can_hold(self.transport)) {
        throw TransportUnviable { transport: self.transport };
      }
      // The minted token (browser) or bearer key (server) authorizes the socket
      // upgrade request identically — the provider doesn't branch on home.
      // authorize declares the CallError channel; CredentialUnavailable also
      // implements RealtimeError, so it survives as a typed RealtimeError here.
      let upgrade = self.auth.authorize(baml.http.Request {
        method: "GET",
        url: "https://api.openai.com/v1/realtime?model=gpt-realtime",
        headers: { "Upgrade": self.transport },
        body: "",
      }) catch (e) {
        let cu: CredentialUnavailable => throw cu;
        _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "realtime upgrade auth failed");
      };
      $rust_io_function     // drive the duplex over `io` using the authorized upgrade
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "openai realtime session failed");
    }
  }
}

// ----------------------------------------------------------------------------
// THE EPHEMERAL-KEY FACTORY — the heart of the browser shape.
//
// The proposal says "a client is a function returning a Provider." So "the server
// mints a session-scoped token, the browser uses that" is simply: a function that
// runs ON THE SERVER, mints a token, and returns a *browser-usable Provider* whose
// auth is that token. No long-lived key ever crosses into the returned provider.
//
// This is a provider FACTORY, not a `client` block, because it does real I/O
// (minting) and is intended to be called server-side and shipped to the browser.
// ----------------------------------------------------------------------------

// The mint endpoint refused (HTTP non-2xx): the server key is invalid, quota is
// exhausted, or the model isn't enabled. A deliberate domain failure on the
// (token-minting) call path — distinct from a transport blip.
class MintRejected {
  status_detail: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
}

// Typed view of the OpenAI realtime client-secret mint response, so the mint
// reads fields off a class (baml.json.from_string) instead of Response.json_path.
class MintResponse {
  value: string
  expires_at: int      // unix SECONDS, per the mint API
}

class SessionMint {
  // Mint a short-lived client secret from OpenAI using the server-held key, then
  // hand back a provider the BROWSER can run. The BearerKey never leaves this fn.
  // Minting is a call-path operation -> the CallError channel.
  function for_browser(server_key: string, voice: string) -> OpenAIRealtime
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let req = baml.http.Request {
      method: "POST",
      url: "https://api.openai.com/v1/realtime/client_secrets",
      headers: {
        "Authorization": "Bearer " + server_key,     // server-only; resolved here, discarded after
        "Content-Type": "application/json",
      },
      body: baml.llm.realtime_mint_body("gpt-realtime"),
    };
    let resp = baml.http.send(req) catch (e) { _ => throw HttpTransportError { detail: "realtime mint send failed" }; };
    if (!resp.ok()) { throw MintRejected { status_detail: resp.text() }; }

    // Decode the mint envelope into a typed class; convert unix seconds -> epoch ms.
    let minted = baml.json.from_string<MintResponse>(resp.text());
    let token = EphemeralToken {
      value: minted.value,
      expires_at_ms: minted.expires_at * 1000,
    };
    // The returned provider carries ONLY the ephemeral token. Safe to serialize to
    // the browser. WebRTC is the browser-viable socket transport.
    OpenAIRealtime { voice: voice, auth: token, transport: "webrtc" }
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "realtime token mint failed");
  }
}

// ----------------------------------------------------------------------------
// A proxy combinator — the OTHER safe browser pattern for plain HTTP providers
// that have NO ephemeral-key support. The browser holds no key at all; it talks
// to your server, which forwards to the real provider. Modeled as a Provider that
// wraps a relative URL and lets the home's reverse proxy attach the real auth.
// ----------------------------------------------------------------------------

class ProxiedThrough {
  // The same-origin path the browser POSTs to; the server behind it injects auth.
  proxy_path: string
  // Which output schema/model the server should use (forwarded as a header).
  model: string

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // No Credential here: the browser attaches NOTHING. Auth is the server's job.
      baml.http.Request {
        method: "POST",
        url: self.proxy_path,                       // same-origin, relative
        headers: { "Content-Type": "application/json", "X-Model": self.model },
        body: baml.llm.openai_chat_body<T>(self.model, prompt),
      }
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) { _ => throw HttpTransportError { detail: "proxy http send failed" }; }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "proxy parse failed");
    }
  }
  // SSE-only by construction (it's a same-origin HTTP POST) — viable in a browser.
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.ExtendUnknownError<baml.errors.StreamError> {
      let req = baml.http.Request {
        method: "POST",
        url: self.proxy_path,
        headers: { "Content-Type": "application/json", "X-Model": self.model, "Accept": "text/event-stream" },
        body: baml.llm.openai_chat_body_stream<TStream>(self.model, prompt),
      };
      let sse = baml.http.fetch_sse(req) catch (e) { _ => throw HttpTransportError { detail: "proxy sse failed" }; };
      baml.llm.stream_from_sse<TStream, TFinal>(sse)
    }
  }
}
