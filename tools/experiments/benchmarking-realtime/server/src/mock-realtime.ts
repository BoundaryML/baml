// Minimal mock of the OpenAI Realtime GA WebSocket protocol, used to exercise
// the relay's delegate→BAML→function_call_output loop without an API key.
// It answers every user text message with a filler-phrase turn that includes a
// function_call to delegate(), then echoes the function_call_output back as a
// final text turn.

import { WebSocketServer } from "ws";

const PORT = Number(process.env.MOCK_PORT ?? 8790);
const wss = new WebSocketServer({ port: PORT });
let callSeq = 0;

wss.on("connection", (ws) => {
  const send = (e: Record<string, any>) => ws.send(JSON.stringify(e));
  send({ type: "session.created" });

  ws.on("message", (raw) => {
    const event = JSON.parse(raw.toString());
    switch (event.type) {
      case "session.update":
        send({ type: "session.updated", session: event.session });
        break;
      case "conversation.item.create": {
        const item = event.item;
        if (item.type === "message" && item.role === "user") {
          // Turn 1: filler + tool call, mimicking the prompting-guide pattern.
          const text = item.content?.[0]?.text ?? "";
          const callId = `call_mock_${++callSeq}`;
          send({ type: "response.created" });
          send({ type: "response.output_text.delta", delta: "One sec." });
          send({ type: "response.output_text.done", text: "One sec." });
          send({
            type: "response.done",
            response: {
              output: [
                { type: "message" },
                {
                  type: "function_call",
                  name: "delegate",
                  call_id: callId,
                  arguments: JSON.stringify({ request: text }),
                },
              ],
            },
          });
        } else if (item.type === "function_call_output") {
          // Turn 2: "speak" the tool result.
          const outcome = JSON.parse(item.output);
          const reply = `[voice would say] ${outcome.say}`;
          send({ type: "response.created" });
          send({ type: "response.output_text.delta", delta: reply });
          send({ type: "response.output_text.done", text: reply });
          send({ type: "response.done", response: { output: [{ type: "message" }] } });
        }
        break;
      }
      case "response.create":
        break; // turns are driven by item.create above
    }
  });
});

console.log(`mock realtime server on ws://localhost:${PORT}`);
