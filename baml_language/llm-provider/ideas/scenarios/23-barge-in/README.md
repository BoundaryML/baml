# 23 — Barge-in, interruption & mid-session mutation

A user starts talking over the model. The app must cut the model off *now* (`response.cancel`), rewrite the server's memory so it reflects only what the user actually *heard* (`conversation.item.truncate` at the played-ms position), and — separately — change voice/instructions/tools mid-call (`session.update`), all over the *same* open `Channel` while a response is in flight. This scenario models those control moves as a net-new `LiveControl requires Realtime` capability whose methods the caller sends *into* `io`, threads played-position bookkeeping through an app-owned `PlaybackCursor` (which also drops `audio.delta` frames that race in after a cancel), and contrasts OpenAI's ms-truncate against Gemini Live's server-authoritative discard — where `truncate` honestly degrades to a no-op and the abstraction's seams show.

Background: background/04-realtime-and-transports.md → ## ◆ Barge-in, interruption & mid-session mutation
