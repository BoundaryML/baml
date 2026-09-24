// Demo program for the durable functions proof of concept.
//
// A function whose name contains `durable` gets automatic snapshots and crash
// recovery. A call to a function whose name starts with `remote_` runs as its
// own run on another machine of the remote pool. Under a plain `baml run` both
// markers have no effect and every call is an ordinary local call.

class TripPlan {
    city: string,
    ideas: string[],
    weather: string,
}

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
        day += 1;
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
        day += 1;
    }
    let weather = remote_fetch_weather(city);
    TripPlan { city: city, ideas: ideas, weather: weather }
}

function durable_plan_trip_parallel(city: string) -> TripPlan {
    let ideas: string[] = [];
    baml.io.println("starting the weather lookup in the background");
    let weather_future = spawn { remote_fetch_weather(city) };
    let day = 1;
    while (day < 4) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1500n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1;
    }
    baml.io.println("waiting for the weather");
    let weather = await weather_future;
    TripPlan { city: city, ideas: ideas, weather: weather }
}
