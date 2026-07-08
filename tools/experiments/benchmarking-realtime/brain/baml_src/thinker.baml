// The "thinker" half of OpenAI's responder–thinker realtime pattern:
// gpt-realtime-2.1 speaks; this BAML function decides which tool to run and
// with what arguments. Model/endpoint are env-overridable so the same brain
// can point at any OpenAI-compatible endpoint (THINKER_* in server/.env).

client<llm> Thinker {
  provider openai
  options {
    model env.THINKER_MODEL
    base_url env.THINKER_BASE_URL
    api_key env.THINKER_API_KEY
  }
}

function DecideAction(history: string, request: string, now_iso: string) -> ThinkerAction {
  client Thinker
  prompt #"
    You are the reasoning engine behind a voice assistant. The voice model has
    delegated the user's latest request to you. Decide the single best action.

    Current local datetime: {{ now_iso }}

    Rules:
    - Order IDs are precise alphanumerics: normalize to uppercase and keep
      dashes exactly as spoken (e.g. "a1b2 dash c3d4" -> "A1B2-C3D4").
    - Timer durations must be converted to whole seconds ("an hour and a
      half" -> 5400).
    - For weather, extract the city name only; put any country/state
      disambiguation in region.
    - Calendar events: resolve relative dates ("next tuesday at 3pm") against
      the current datetime above into a local ISO start_iso. Default duration
      30 minutes unless stated. Attendee names exactly as spoken.
    - Messages: keep the body natural first-person text ready to send; pick
      urgency from tone ("let them know whenever" -> low, "right now" -> urgent).
    - Medication plans: do the arithmetic yourself. tablets_per_dose from
      spoken fractions ("two and a half" -> 2.5, "half a tablet" -> 0.5).
      doses_total = total course hours / interval_hours ("every eight hours
      for ten days" -> 240 / 8 = 30). Resolve first_dose_iso against the
      current datetime ("tomorrow morning at eight" -> next day 08:00).
    - If no tool fits, Answer directly in one or two spoken-style sentences.

    Conversation so far:
    {{ history }}

    Latest user request:
    {{ request }}

    {{ ctx.output_format }}
  "#
}

/// Entry point the Node relay calls: decide, execute, and package a result
/// the realtime model can speak back.
function handle(history: string, request: string, now_iso: string) -> ToolOutcome {
  let action = DecideAction(history, request, now_iso) catch (e) {
    _ => {
      return ToolOutcome {
        say: "Sorry, I hit a problem thinking that through. Could you rephrase?",
        tool: "error",
        data: null,
      };
    }
  };
  execute(action)
}

function execute(action: ThinkerAction) -> ToolOutcome {
  match (action) {
    let a: GetWeather => run_weather(a),
    let a: LookupOrder => run_order_lookup(a),
    let a: SetTimer => run_timer(a),
    let a: CreateCalendarEvent => run_calendar(a),
    let a: SendMessage => run_send_message(a),
    let a: PlanMedication => run_medication(a),
    let a: Answer => ToolOutcome {
      say: a.message,
      tool: "answer",
      data: null,
    },
  }
}
