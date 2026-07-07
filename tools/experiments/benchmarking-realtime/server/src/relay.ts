// Realtime relay: owns the WebSocket to OpenAI's Realtime API (GA protocol)
// and wires its single "delegate" function tool to the BAML thinker.
//
// Responder–thinker split (per OpenAI's realtime prompting guide):
//   gpt-realtime-2.1 talks and listens; every actionable request is delegated
//   to BAML's handle(), which returns a typed ToolOutcome the voice model
//   relays. Tool selection/argument extraction happens in BAML's SAP layer,
//   not in JSON-schema function calling.

import WebSocket from "ws";
import "../baml_sdk/index.js";
import {
  CreateCalendarEvent,
  GetWeather,
  LookupOrder,
  SendMessage,
  SetTimer,
  handle_async,
  run_calendar_async,
  run_order_lookup_async,
  run_send_message_async,
  run_timer_async,
  run_weather_async,
  type ToolOutcome,
} from "../baml_sdk/index.js";
import { spendApiCall } from "./budget.js";

/** Local ISO datetime + weekday, e.g. "2026-07-07T13:25:00 (Tuesday)". */
function nowIso(): string {
  const d = new Date();
  const iso = d.toLocaleString("sv-SE").replace(" ", "T");
  const weekday = d.toLocaleDateString("en-US", { weekday: "long" });
  return `${iso} (${weekday})`;
}


// ---- native-lane argument validation ----------------------------------------
// OpenAI only loosely conforms tool calls to the declared JSON schemas (strict
// mode is off, matching the standard setup most realtime apps ship). Before a
// native call reaches an executor we check it ourselves: parseable JSON, all
// required fields present, right types, enum membership, no unknown fields.
// A malformed call becomes a visible failure (red card, tool: "error") instead
// of being silently coerced into a semantically wrong call.

type FieldRule = {
  type: "string" | "integer" | "string[]";
  required?: boolean;
  nonEmpty?: boolean;
  enum?: string[];
};

const NATIVE_ARG_RULES: Record<string, Record<string, FieldRule>> = {
  get_weather: {
    city: { type: "string", required: true, nonEmpty: true },
    region: { type: "string" },
  },
  lookup_order: {
    order_id: { type: "string", required: true, nonEmpty: true },
  },
  set_timer: {
    seconds: { type: "integer", required: true },
    label: { type: "string" },
  },
  create_calendar_event: {
    title: { type: "string", required: true, nonEmpty: true },
    start_iso: { type: "string", required: true, nonEmpty: true },
    duration_minutes: { type: "integer", required: true },
    attendees: { type: "string[]" },
    location: { type: "string" },
  },
  send_message: {
    to: { type: "string", required: true, nonEmpty: true },
    body: { type: "string", required: true },
    urgency: { type: "string", enum: ["low", "normal", "urgent"] },
  },
};

export function validateNativeArgs(
  name: string,
  rawArgs: string,
): { ok: true; args: Record<string, any> } | { ok: false; problems: string[] } {
  let parsed: any;
  try {
    parsed = JSON.parse(rawArgs);
  } catch (e) {
    return { ok: false, problems: [`arguments are not valid JSON: ${e}`] };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return { ok: false, problems: ["arguments must be a JSON object"] };
  }
  const rules = NATIVE_ARG_RULES[name];
  if (!rules) return { ok: false, problems: [`unknown tool "${name}"`] };
  const problems: string[] = [];
  for (const [field, rule] of Object.entries(rules)) {
    const v = parsed[field];
    if (v === undefined || v === null) {
      if (rule.required) problems.push(`missing required field "${field}"`);
      continue;
    }
    if (rule.type === "string" && typeof v !== "string") {
      problems.push(`"${field}" must be a string, got ${typeof v}`);
    } else if (rule.type === "integer" && (typeof v !== "number" || !Number.isInteger(v))) {
      problems.push(`"${field}" must be an integer, got ${JSON.stringify(v)}`);
    } else if (rule.type === "string[]" && (!Array.isArray(v) || v.some((x: any) => typeof x !== "string"))) {
      problems.push(`"${field}" must be an array of strings`);
    }
    if (rule.nonEmpty && typeof v === "string" && v.trim() === "") {
      problems.push(`"${field}" must not be empty`);
    }
    if (rule.enum && typeof v === "string" && !rule.enum.includes(v)) {
      problems.push(`"${field}" must be one of: ${rule.enum.join(", ")}`);
    }
  }
  for (const k of Object.keys(parsed)) {
    if (!rules[k]) problems.push(`unexpected field "${k}"`);
  }
  return problems.length ? { ok: false, problems } : { ok: true, args: parsed };
}

const INSTRUCTIONS = `
You are a friendly, quick voice assistant.

You have ONE tool: delegate(request, context). A smarter reasoning engine sits
behind it and can check weather, look up orders, set timers, create calendar
events, send text messages to the user's contacts, and answer questions. It can
do ALL of these; never tell the user something on that list is impossible.

Rules:
- For ANY actionable or factual request (weather, order status, timers,
  calendar, messaging, unit math, general knowledge), first say a short natural
  filler like "One sec." or "Let me check.", then IMMEDIATELY call delegate.
  Do not answer from your own knowledge.
- If the user asks for SEVERAL things in one breath, make one delegate call per
  distinct request, in order. Each call's "request" must carry that part
  verbatim WITH any detail mentioned elsewhere in the utterance that it needs
  (city names, IDs, durations, recipients). Keep going until every part has
  been delegated; never drop or refuse a part.
- Pass the user's request verbatim in "request", including spelled-out
  letters/digits (e.g. "a one b two dash c three d four"). Do not normalize
  IDs yourself — the reasoning engine does that.
- When the result arrives, relay its "say" text naturally in your own voice.
  Don't read JSON aloud.
- Only smalltalk and greetings may be answered without the tool.
- Keep every reply to one or two sentences; this is a voice conversation.
`.trim();

const DELEGATE_TOOL = {
  type: "function",
  name: "delegate",
  description:
    "Send ONE of the user's requests to the reasoning engine. It can check weather, look up orders, set timers, create calendar events, send messages to contacts, and answer factual questions. Pass that request verbatim; for multi-part utterances call this once per part.",
  parameters: {
    type: "object",
    properties: {
      request: {
        type: "string",
        description:
          "The user's latest request, verbatim, including any spelled-out letters and digits.",
      },
      context: {
        type: "string",
        description:
          "Optional: anything from earlier in the conversation the engine needs (prior IDs, cities, corrections).",
      },
    },
    required: ["request"],
  },
};

// ---- native mode: the standard realtime setup (tools declared as JSON
// schemas in session.tools; the realtime model itself selects tools and
// extracts arguments). Executors are shared with delegate mode, so any output
// difference is purely tool-calling quality.

const NATIVE_INSTRUCTIONS = `
You are a friendly, quick voice assistant. Use the provided tools when the
user asks about weather, order status, timers, calendar events, or sending
messages; otherwise answer directly. Keep every reply to one or two
sentences; this is a voice conversation.
`.trim();

const NATIVE_TOOLS = [
  {
    type: "function",
    name: "get_weather",
    description: "Get the current weather for a city.",
    parameters: {
      type: "object",
      properties: {
        city: { type: "string", description: "City name." },
        region: {
          type: "string",
          description: "State or country qualifier when the city is ambiguous.",
        },
      },
      required: ["city"],
    },
  },
  {
    type: "function",
    name: "lookup_order",
    description: "Look up a customer order by its alphanumeric ID, e.g. A1B2-C3D4.",
    parameters: {
      type: "object",
      properties: {
        order_id: { type: "string", description: "The order ID, e.g. A1B2-C3D4." },
      },
      required: ["order_id"],
    },
  },
  {
    type: "function",
    name: "set_timer",
    description: "Start a countdown timer on the user's device.",
    parameters: {
      type: "object",
      properties: {
        seconds: { type: "integer", description: "Duration in whole seconds." },
        label: { type: "string", description: "Optional timer label." },
      },
      required: ["seconds"],
    },
  },
  {
    type: "function",
    name: "create_calendar_event",
    description: "Put an event on the user's calendar.",
    parameters: {
      type: "object",
      properties: {
        title: { type: "string" },
        start_iso: {
          type: "string",
          description: "Local ISO 8601 start, e.g. 2026-07-14T15:00:00.",
        },
        duration_minutes: { type: "integer" },
        attendees: { type: "array", items: { type: "string" } },
        location: { type: "string" },
      },
      required: ["title", "start_iso", "duration_minutes"],
    },
  },
  {
    type: "function",
    name: "send_message",
    description: "Send a text message to a contact by name.",
    parameters: {
      type: "object",
      properties: {
        to: { type: "string", description: "Contact name, e.g. alice, mom." },
        body: { type: "string" },
        urgency: { type: "string", enum: ["low", "normal", "urgent"] },
      },
      required: ["to", "body"],
    },
  },
];

export interface RelayEvents {
  /** Raw server events, for logging/forwarding. */
  onEvent?: (event: Record<string, any>) => void;
  /** Base64 PCM16 audio chunks from the assistant. */
  onAudio?: (b64: string) => void;
  /** Incremental assistant text/transcript. */
  onTextDelta?: (text: string) => void;
  /** A completed assistant utterance. */
  onTextDone?: (text: string) => void;
  /** A tool round-trip finished (already reported back to the model). */
  onToolResult?: (outcome: ToolOutcome, args: Record<string, any>, toolName: string) => void;
  /** A response finished with no pending tool round-trip: the turn is settled. */
  onSettled?: () => void;
  /** Server VAD detected end of user speech (voice turn starting). */
  onSpeechStopped?: () => void;
  onError?: (message: string) => void;
  onReady?: () => void;
}

export interface RelayOptions {
  apiKey: string;
  model?: string;
  /** "audio" for voice, "text" for the CLI harness. */
  outputModality?: "audio" | "text";
  /** "delegate" (BAML thinker, default) or "native" (standard session.tools). */
  mode?: "delegate" | "native";
  voice?: string;
  events?: RelayEvents;
}

export class RealtimeRelay {
  private ws: WebSocket;
  private events: RelayEvents;
  private history: { role: string; text: string }[] = [];
  private outputModality: "audio" | "text";
  private mode: "delegate" | "native";
  private voice: string;

  constructor(private opts: RelayOptions) {
    this.events = opts.events ?? {};
    this.outputModality = opts.outputModality ?? "audio";
    this.mode = opts.mode ?? "delegate";
    this.voice = opts.voice ?? "marin";
    const model = opts.model ?? process.env.REALTIME_MODEL ?? "gpt-realtime-2.1";
    const base = process.env.REALTIME_URL ?? "wss://api.openai.com/v1/realtime";
    if (base.includes("api.openai.com")) {
      spendApiCall(`${model} ${this.outputModality}`);
    }
    this.ws = new WebSocket(`${base}?model=${model}`, {
      headers: { Authorization: `Bearer ${opts.apiKey}` },
    });
    this.ws.on("open", () => this.configureSession());
    this.ws.on("message", (raw) => this.onServerEvent(JSON.parse(raw.toString())));
    this.ws.on("error", (err) => this.events.onError?.(String(err)));
    this.ws.on("close", (code, reason) =>
      this.events.onError?.(`realtime socket closed: ${code} ${reason}`),
    );
  }

  private send(event: Record<string, any>) {
    this.ws.send(JSON.stringify(event));
  }

  private configureSession() {
    const session: Record<string, any> = {
      type: "realtime",
      instructions:
        (this.mode === "native" ? NATIVE_INSTRUCTIONS : INSTRUCTIONS) +
        `\n\nCurrent local datetime: ${nowIso()}.`,
      output_modalities: [this.outputModality],
      tools: this.mode === "native" ? NATIVE_TOOLS : [DELEGATE_TOOL],
      tool_choice: "auto",
    };
    if (this.outputModality === "audio") {
      session.audio = { output: { voice: this.voice } };
    }
    this.send({ type: "session.update", session });
  }

  /** Base64 PCM16 mic audio from the browser client. */
  appendAudio(b64: string) {
    this.send({ type: "input_audio_buffer.append", audio: b64 });
  }

  /** Text-mode input (CLI harness). */
  sendText(text: string) {
    this.history.push({ role: "user", text });
    this.send({
      type: "conversation.item.create",
      item: {
        type: "message",
        role: "user",
        content: [{ type: "input_text", text }],
      },
    });
    this.send({ type: "response.create" });
  }

  close() {
    this.ws.close();
  }

  private historyText(): string {
    return this.history
      .slice(-12)
      .map((h) => `${h.role}: ${h.text}`)
      .join("\n");
  }

  private async onServerEvent(event: Record<string, any>) {
    this.events.onEvent?.(event);
    switch (event.type) {
      case "session.updated":
        this.events.onReady?.();
        break;
      case "input_audio_buffer.speech_stopped":
        this.events.onSpeechStopped?.();
        break;
      case "response.output_audio.delta":
        this.events.onAudio?.(event.delta);
        break;
      case "response.output_text.delta":
      case "response.output_audio_transcript.delta":
        this.events.onTextDelta?.(event.delta);
        break;
      case "response.output_text.done":
        this.recordAssistant(event.text);
        break;
      case "response.output_audio_transcript.done":
        this.recordAssistant(event.transcript);
        break;
      case "response.done": {
        const calls = (event.response?.output ?? []).filter(
          (item: any) => item.type === "function_call",
        );
        if (calls.length === 0) {
          this.events.onSettled?.();
          break;
        }
        for (const call of calls) {
          if (this.mode === "native") {
            await this.runNativeTool(call.call_id, call.name, call.arguments);
          } else if (call.name === "delegate") {
            await this.runDelegate(call.call_id, call.arguments);
          }
        }
        this.send({ type: "response.create" });
        break;
      }
      case "error":
        this.events.onError?.(JSON.stringify(event.error ?? event));
        break;
    }
  }

  private recordAssistant(text: string | undefined) {
    if (!text) return;
    this.history.push({ role: "assistant", text });
    this.events.onTextDone?.(text);
  }

  private async runDelegate(callId: string, rawArgs: string) {
    let args: Record<string, any> = {};
    try {
      args = JSON.parse(rawArgs);
    } catch {
      // fall through with the raw string as the request
      args = { request: rawArgs };
    }
    const request = [args.request, args.context && `(context: ${args.context})`]
      .filter(Boolean)
      .join(" ");
    this.history.push({ role: "user", text: request });

    let outcome: ToolOutcome;
    try {
      outcome = await handle_async(this.historyText(), request, nowIso());
    } catch (err) {
      outcome = {
        say: "The reasoning engine had a problem with that one.",
        tool: "error",
        data: null,
      } as ToolOutcome;
      this.events.onError?.(`baml handle() failed: ${err}`);
    }

    this.events.onToolResult?.(outcome, args, "delegate");
    this.sendToolOutput(callId, outcome);
  }

  /** Native mode: the realtime model picked the tool and extracted the args;
   *  we map them onto the same BAML executors delegate mode uses. */
  private async runNativeTool(callId: string, name: string, rawArgs: string) {
    this.history.push({ role: "user", text: `[${name}] ${rawArgs}` });

    // formatting gate: malformed arguments fail loudly instead of being coerced
    const checked = validateNativeArgs(name, rawArgs);
    if (!checked.ok) {
      const outcome = {
        say: `The ${name} call was malformed, so I couldn't run it.`,
        tool: "error",
        data: { error: "malformed_arguments", tool: name, problems: checked.problems, raw_arguments: rawArgs },
      } as ToolOutcome;
      this.events.onError?.(`native tool ${name} malformed: ${checked.problems.join("; ")}`);
      this.events.onToolResult?.(outcome, { raw: rawArgs }, name);
      this.sendToolOutput(callId, outcome);
      return;
    }
    const args = checked.args;

    let outcome: ToolOutcome;
    try {
      // NOTE: call the monomorphic executors, not execute_async(union) — the
      // generated SDK currently mis-tags union-typed params as the first
      // variant (SetTimer arrives as GetWeather and panics on missing fields).
      if (name === "get_weather") {
        outcome = await run_weather_async(
          new GetWeather({ city: String(args.city ?? ""), region: args.region != null ? String(args.region) : null }),
        );
      } else if (name === "lookup_order") {
        outcome = await run_order_lookup_async(new LookupOrder({ order_id: String(args.order_id ?? "") }));
      } else if (name === "set_timer") {
        outcome = await run_timer_async(
          new SetTimer({ seconds: Number(args.seconds ?? 0), label: args.label != null ? String(args.label) : null }),
        );
      } else if (name === "create_calendar_event") {
        outcome = await run_calendar_async(
          new CreateCalendarEvent({
            title: String(args.title ?? ""),
            start_iso: String(args.start_iso ?? ""),
            duration_minutes: Number(args.duration_minutes ?? 30),
            attendees: Array.isArray(args.attendees) ? args.attendees.map(String) : [],
            location: args.location != null ? String(args.location) : null,
          }),
        );
      } else if (name === "send_message") {
        const urgency = ["low", "normal", "urgent"].includes(args.urgency) ? args.urgency : "normal";
        outcome = await run_send_message_async(
          new SendMessage({ to: String(args.to ?? ""), body: String(args.body ?? ""), urgency }),
        );
      } else {
        outcome = { say: `I don't have a tool called ${name}.`, tool: "error", data: null } as ToolOutcome;
      }
    } catch (err) {
      outcome = { say: "That tool failed.", tool: "error", data: null } as ToolOutcome;
      this.events.onError?.(`native tool ${name} failed: ${err}`);
    }

    this.events.onToolResult?.(outcome, args, name);
    this.sendToolOutput(callId, outcome);
  }

  private sendToolOutput(callId: string, outcome: ToolOutcome) {
    this.send({
      type: "conversation.item.create",
      item: {
        type: "function_call_output",
        call_id: callId,
        output: JSON.stringify(outcome),
      },
    });
  }
}
