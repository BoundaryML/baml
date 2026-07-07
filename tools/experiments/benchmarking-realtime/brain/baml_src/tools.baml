// Tool executors. Weather is a real HTTP integration (open-meteo, keyless);
// orders are a fixture DB; timers execute client-side so we just normalize
// and acknowledge.

// ---------- weather (open-meteo) ----------

class GeoResult {
  name string
  latitude float
  longitude float
  country string?
  admin1 string?
}

class GeoResponse {
  results GeoResult[]?
}

class CurrentWeather {
  temperature_2m float
  wind_speed_10m float
  weather_code float
}

class ForecastResponse {
  current CurrentWeather
}

function weather_code_label(code: int) -> string {
  match (code) {
    0 => "clear skies",
    1 => "mostly clear",
    2 => "partly cloudy",
    3 => "overcast",
    45 => "foggy",
    48 => "foggy",
    51 => "light drizzle",
    61 => "light rain",
    63 => "rain",
    65 => "heavy rain",
    71 => "light snow",
    73 => "snow",
    75 => "heavy snow",
    80 => "rain showers",
    95 => "thunderstorms",
    _ => "mixed conditions",
  }
}

function run_weather(args: GetWeather) -> ToolOutcome {
  let query = match (args.region) {
    null => args.city,
    let r: string => `${args.city}, ${r}`,
  };
  let outcome = fetch_weather(args.city, args.region) catch (e) {
    _ => {
      return ToolOutcome {
        say: `I couldn't reach the weather service for ${query} just now.`,
        tool: "error",
        data: null,
      };
    }
  };
  outcome
}

/// Pick the first geocoding hit whose admin1 (state) or country mentions the
/// requested region; falls back to the top hit when nothing matches.
function pick_place(results: GeoResult[], region: string?) -> GeoResult? {
  match (region) {
    null => results.at(0),
    let r: string => {
      let want = r.to_lower_case();
      for (let cand in results) {
        let admin = match (cand.admin1) {
          null => "",
          let a: string => a.to_lower_case(),
        };
        let country = match (cand.country) {
          null => "",
          let c: string => c.to_lower_case(),
        };
        if (admin.includes(want) || country.includes(want) || want.includes(admin) && admin != "") {
          return cand;
        }
      }
      results.at(0)
    },
  }
}

function fetch_weather(city: string, region: string?) -> ToolOutcome throws baml.errors.Io | baml.errors.Timeout | baml.json.JsonParseError | baml.json.JsonDecodeError | baml.errors.InvalidArgument {
  let encoded = city.replace(" ", "+");
  let geo_resp = baml.http.fetch(
    `https://geocoding-api.open-meteo.com/v1/search?name=${encoded}&count=10`,
    timeout = baml.time.Duration.from_milliseconds(8000),
  );
  let geo = baml.json.from_string<GeoResponse>(geo_resp.text());
  match (geo.results) {
    null => ToolOutcome {
      say: `I couldn't find a city called ${city}.`,
      tool: "weather",
      data: null,
    },
    let results: GeoResult[] => {
      match (pick_place(results, region)) {
        null => ToolOutcome {
          say: `I couldn't find a city called ${city}.`,
          tool: "weather",
          data: null,
        },
        let place: GeoResult => {
          let fc_resp = baml.http.fetch(
            `https://api.open-meteo.com/v1/forecast?latitude=${place.latitude}&longitude=${place.longitude}&current=temperature_2m,weather_code,wind_speed_10m`,
            timeout = baml.time.Duration.from_milliseconds(8000),
          );
          let fc = baml.json.from_string<ForecastResponse>(fc_resp.text());
          let label = weather_code_label(fc.current.weather_code.iround());
          let temp = fc.current.temperature_2m.iround();
          let spoken_place = match (place.admin1) {
            null => place.name,
            let a: string => `${place.name}, ${a}`,
          };
          let d: map<string, json> = {};
          d.set("city", spoken_place);
          d.set("temperature_c", fc.current.temperature_2m);
          d.set("wind_kmh", fc.current.wind_speed_10m);
          d.set("conditions", label);
          ToolOutcome {
            say: `In ${spoken_place} it's currently ${temp} degrees Celsius with ${label}.`,
            tool: "weather",
            data: d,
          }
        },
      }
    },
  }
}

// ---------- order lookup (fixture DB) ----------

class OrderInfo {
  status string
  item string
  eta string
}

function order_db() -> map<string, OrderInfo> {
  let db: map<string, OrderInfo> = {};
  db.set("A1B2-C3D4", OrderInfo { status: "shipped", item: "mechanical keyboard", eta: "July 9" });
  db.set("Z9Y8-X7W6", OrderInfo { status: "processing", item: "standing desk", eta: "July 15" });
  db.set("Q5R6-S7T8", OrderInfo { status: "delivered", item: "USB-C dock", eta: "delivered July 2" });
  db
}

function run_order_lookup(args: LookupOrder) -> ToolOutcome {
  let id = args.order_id.to_upper_case();
  match (order_db().get(id)) {
    null => ToolOutcome {
      say: `I couldn't find an order with ID ${id}. Could you double-check it?`,
      tool: "order",
      data: null,
    },
    let o: OrderInfo => {
      let d: map<string, json> = {};
      d.set("order_id", id);
      d.set("status", o.status);
      d.set("item", o.item);
      d.set("eta", o.eta);
      ToolOutcome {
        say: `Order ${id}, your ${o.item}, is ${o.status} with expected arrival ${o.eta}.`,
        tool: "order",
        data: d,
      }
    },
  }
}

// ---------- timer (executed client-side) ----------

function plural(n: int, unit: string) -> string {
  if (n == 1) {
    `1 ${unit}`
  } else {
    `${n} ${unit}s`
  }
}

function speakable_duration(seconds: int) -> string {
  if (seconds >= 3600) {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    if (m > 0) {
      `${plural(h, "hour")} and ${plural(m, "minute")}`
    } else {
      plural(h, "hour")
    }
  } else if (seconds >= 60) {
    let m = seconds / 60;
    let s = seconds % 60;
    if (s > 0) {
      `${plural(m, "minute")} and ${plural(s, "second")}`
    } else {
      plural(m, "minute")
    }
  } else {
    plural(seconds, "second")
  }
}

function run_timer(args: SetTimer) -> ToolOutcome {
  if (args.seconds <= 0) {
    return ToolOutcome {
      say: "That timer duration didn't make sense to me — how long should it run?",
      tool: "error",
      data: null,
    };
  }
  let d: map<string, json> = {};
  d.set("seconds", args.seconds);
  match (args.label) {
    null => null,
    let l: string => { d.set("label", l); null },
  };
  ToolOutcome {
    say: `Timer set for ${speakable_duration(args.seconds)}.`,
    tool: "timer",
    data: d,
  }
}

// ---------- calendar (client-side effect; we validate + speak it back) ----------

function month_name(m: int) -> string {
  match (m) {
    1 => "January", 2 => "February", 3 => "March", 4 => "April",
    5 => "May", 6 => "June", 7 => "July", 8 => "August",
    9 => "September", 10 => "October", 11 => "November", 12 => "December",
    _ => "month ${m}",
  }
}

/// "2026-07-14T15:00:00" -> "July 14 at 3:00 PM"; falls back to the raw
/// string when the shape is unexpected.
function speakable_datetime(iso: string) -> string {
  let parts = iso.split("T");
  match (parts.at(1)) {
    null => iso,
    let time: string => {
      match (parts.at(0)) {
        null => iso,
        let date: string => {
          let d = date.split("-");
          let t = time.split(":");
          let month = match (d.at(1)) { null => 0, let x: string => int.parse(x) catch (_) { _ => 0 } };
          let day = match (d.at(2)) { null => 0, let x: string => int.parse(x) catch (_) { _ => 0 } };
          let hour = match (t.at(0)) { null => 0, let x: string => int.parse(x) catch (_) { _ => 0 } };
          let minute = match (t.at(1)) { null => "00", let x: string => x };
          if (month == 0 || day == 0) {
            return iso;
          }
          let ampm = if (hour >= 12) { "PM" } else { "AM" };
          let h12 = if (hour % 12 == 0) { 12 } else { hour % 12 };
          `${month_name(month)} ${day} at ${h12}:${minute} ${ampm}`
        },
      }
    },
  }
}

function run_calendar(args: CreateCalendarEvent) -> ToolOutcome {
  if (args.duration_minutes <= 0) {
    return ToolOutcome {
      say: "That event duration didn't make sense to me; how long should it be?",
      tool: "error",
      data: null,
    };
  }
  let when = speakable_datetime(args.start_iso);
  let who = match (args.attendees.length()) {
    0 => "",
    _ => ` with ${args.attendees.join(", ")}`,
  };
  let d: map<string, json> = {};
  d.set("title", args.title);
  d.set("start_iso", args.start_iso);
  d.set("duration_minutes", args.duration_minutes);
  d.set("attendees", args.attendees);
  match (args.location) {
    null => null,
    let l: string => { d.set("location", l); null },
  };
  ToolOutcome {
    say: `Booked "${args.title}"${who} on ${when} for ${args.duration_minutes} minutes.`,
    tool: "calendar",
    data: d,
  }
}

// ---------- messaging (fixture contact book) ----------

function contact_book() -> map<string, string> {
  let c: map<string, string> = {};
  c.set("alice", "+1-555-0100");
  c.set("bob", "+1-555-0101");
  c.set("mom", "+1-555-0102");
  c.set("dr chen", "+1-555-0103");
  c
}

function run_send_message(args: SendMessage) -> ToolOutcome {
  let key = args.to.to_lower_case().replace(".", "").trim();
  match (contact_book().get(key)) {
    null => ToolOutcome {
      say: `I don't have a contact called ${args.to}. I know alice, bob, mom, and dr chen.`,
      tool: "message",
      data: null,
    },
    let number: string => {
      let d: map<string, json> = {};
      d.set("to", key);
      d.set("number", number);
      d.set("body", args.body);
      d.set("urgency", args.urgency);
      let urgency_note = match (args.urgency) {
        "urgent" => " marked urgent",
        "low" => "",
        _ => "",
      };
      ToolOutcome {
        say: `Message sent to ${args.to}${urgency_note}: "${args.body}"`,
        tool: "message",
        data: d,
      }
    },
  }
}
