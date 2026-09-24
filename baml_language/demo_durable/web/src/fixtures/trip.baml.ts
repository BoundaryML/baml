/**
 * The demo program that the fixtures refer to. It follows section 4 of the
 * contracts. The copy in `demo_durable/program/` is the one that the site
 * servers run. This copy only serves the source view in fixture mode.
 */
export const TRIP_BAML_FILE = "baml_src/trip.baml";

export const TRIP_BAML = `class TripPlan {
    city: string
    ideas: string[]
    weather: string
}

// Runs on a machine of the remote pool: the \`remote_\` prefix marks a remote call.
function remote_fetch_weather(city: string) -> string {
    baml.io.println("[remote] looking up weather for " + city);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(3000n));
    "sunny in " + city
}

function durable_plan_trip(city: string) -> TripPlan {
    let ideas: string[] = [];
    let day = 1;
    while (day < 4) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1500n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1
    }
    let weather = remote_fetch_weather(city);
    TripPlan { city: city, ideas: ideas, weather: weather }
}

function plan_trip(city: string) -> TripPlan {
    let ideas: string[] = [];
    let day = 1;
    while (day < 4) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1500n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1
    }
    let weather = remote_fetch_weather(city);
    TripPlan { city: city, ideas: ideas, weather: weather }
}

function durable_plan_trip_parallel(city: string) -> TripPlan {
    let forecast = spawn { remote_fetch_weather(city) };
    let ideas: string[] = [];
    let day = 1;
    while (day < 5) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1500n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1
    }
    let weather = await forecast;
    TripPlan { city: city, ideas: ideas, weather: weather }
}
`;

/**
 * The 1-based line of the first occurrence of `needle` inside the body of the
 * function `fn`. Fixtures use this instead of literal line numbers, so an edit of the
 * program text cannot leave them stale.
 */
export function lineOf(fn: string, needle: string): number {
  const lines = TRIP_BAML.split("\n");
  const start = lines.findIndex((line) => line.startsWith(`function ${fn}(`));
  if (start === -1) throw new Error(`fixture program has no function ${fn}`);
  // The search starts after the signature line, which can contain the needle (a return type, for example).
  for (let i = start + 1; i < lines.length; i++) {
    const line = lines[i] as string;
    if (line.startsWith("}")) break;
    if (line.includes(needle)) return i + 1;
  }
  throw new Error(`function ${fn} has no line that contains ${needle}`);
}
