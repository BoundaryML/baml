// Typed action space for the thinker. The realtime (voice) model delegates
// here; SAP parses the thinker LLM's reply directly into one of these classes,
// so tool selection and argument extraction are a single structured decode —
// no JSON-schema function-calling layer involved.

/// Look up current weather for a city (open-meteo, no API key).
class GetWeather {
  city string
  /// Optional country or region qualifier when the city is ambiguous.
  region string?
}

/// Look up a customer order by its exact alphanumeric ID (e.g. "A1B2-C3D4").
class LookupOrder {
  /// Must be the exact ID the user spelled out, uppercased, with dashes.
  order_id string
}

/// Start a countdown timer on the user's device.
class SetTimer {
  seconds int
  label string?
}

/// Put an event on the user's calendar. Relative dates ("next tuesday at 3")
/// must be resolved against the provided current datetime.
class CreateCalendarEvent {
  title string
  /// ISO 8601 local datetime, e.g. "2026-07-14T15:00:00".
  start_iso string
  duration_minutes int
  /// Names exactly as the user said them; may be empty.
  attendees string[]
  location string?
}

/// Send a message to a contact by name.
class SendMessage {
  /// Contact name as spoken, e.g. "mom", "alice".
  to string
  body string
  urgency "low" | "normal" | "urgent"
}

/// Plan a recurring medication course. The hard part is arithmetic the
/// speaker never states directly: doses_total must be COMPUTED from the
/// course length and interval ("every eight hours for ten days" -> 30), and
/// tablets_per_dose comes from spoken fractions ("two and a half" -> 2.5).
/// first_dose_iso resolves relative speech against the current datetime.
class PlanMedication {
  drug string
  /// Tablets per single dose; spoken fractions become decimals ("half" -> 0.5).
  tablets_per_dose float
  /// Hours between doses ("every eight hours" -> 8).
  interval_hours int
  /// TOTAL number of doses over the whole course: course duration divided by
  /// the interval ("every 8 hours for 10 days" -> 240 / 8 = 30).
  doses_total int
  /// ISO 8601 local datetime of the first dose, resolved from relative
  /// speech ("starting tomorrow morning at eight") against the current time.
  first_dose_iso string
}

/// No tool needed: the thinker answers directly from knowledge/conversation.
class Answer {
  message string
}

type ThinkerAction = GetWeather | LookupOrder | SetTimer | CreateCalendarEvent | SendMessage | PlanMedication | Answer

/// What goes back to the realtime model (and the client UI).
class ToolOutcome {
  /// Concise, speakable summary the voice model should relay to the user.
  say string
  /// Which path ran: "weather" | "order" | "timer" | "answer" | "error".
  tool string
  /// Structured payload for client-side effects (timer config, raw data).
  data json
}
