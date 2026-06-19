# Deployment shapes — server / browser / edge / durable

One provider layer, four homes. Where the LLM call runs dictates where the secret comes from (env var, platform binding, cloud-IAM chain, or a session-scoped token the server mints for the browser), how long a connection can live (SSE works everywhere; WebSocket/WebRTC need a socket-holding host like a server or durable object), and who holds the long-lived key. This scenario shows the proposal absorbing all of that by leaning on "a client is a function returning a Provider" (so "which home" is a function call, and the ephemeral-key pattern is a provider factory) and a new `Credential` capability that hides static-key / IAM / minted- token behind one `authorize` — while flagging the two things it genuinely cannot guarantee: bundle-time secret hygiene and transport viability as a type.

Background: background/05-cross-cutting.md → ## ◆ Deployment shapes
