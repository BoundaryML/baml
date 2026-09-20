class TripPlan {
    city: string,
    ideas: string[],
    weather: string,
}

function remote_fetch_weather(city: string) -> string {
    baml.io.println("[cloud] looking up weather for " + city);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(300n));
    "sunny in " + city
}

function durable_plan_trip(city: string) -> TripPlan {
    let ideas: string[] = [];
    let day = 1;
    while (day < 4) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1
    }
    let weather = remote_fetch_weather(city);
    TripPlan { city: city, ideas: ideas, weather: weather }
}

function durable_plan_trip_parallel(city: string) -> TripPlan {
    let ideas: string[] = [];
    let pending = spawn { remote_fetch_weather(city) };
    let day = 1;
    while (day < 4) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1
    }
    let weather = await pending;
    TripPlan { city: city, ideas: ideas, weather: weather }
}

function durable_safe_weather(city: string) -> string {
    {
        remote_fetch_weather(city)
    } catch (e) {
        baml.errors.Io => "unknown"
    }
}

function durable_detached(city: string) -> string {
    let late = spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
        baml.io.println("too late for " + city);
        1
    };
    "done early"
}

function durable_slow_trip(city: string) -> TripPlan {
    let ideas: string[] = [];
    let day = 1;
    while (day < 4) {
        baml.io.println("planning day " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(700n));
        ideas.push("day " + day.to_string() + " in " + city);
        day += 1
    }
    let weather = remote_fetch_weather(city);
    TripPlan { city: city, ideas: ideas, weather: weather }
}

function durable_spin(city: string) -> int {
    let i = 0;
    let sum = 0;
    while (i < 4000000) {
        sum += i * 2;
        i += 1;
    }
    baml.io.println("spun for " + city);
    sum
}

function bump(x: int) -> int {
    x + 1
}

function durable_spin_calls(city: string) -> int {
    let i = 0;
    let total = 0;
    while (i < 500000) {
        let label = "i=" + i.to_string();
        total = bump(total) + bump(label.length()) - bump(label.length());
        i = bump(i);
    }
    baml.io.println("spun with calls for " + city);
    total
}

function durable_endless(city: string) -> int {
    let i = 0;
    let laps = 0;
    while (laps < 2000000000) {
        i += 1;
        if (i > 1000000) {
            i = 0;
            laps += 1;
        }
    }
    laps
}

function kick(city: string) -> int {
    let forgotten = spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(600n));
        1
    };
    0
}

function durable_forgetful(city: string) -> string {
    kick(city);
    let day = 1;
    while (day < 8) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(250n));
        day += 1
    }
    city + " after " + day.to_string()
}

function durable_split_line(city: string) -> string {
    baml.io.print("hello ");
    baml.sys.sleep(baml.time.Duration.from_milliseconds(700n));
    baml.io.println(city);
    "done"
}
