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

// Phase 3: a race over remote calls. The losers are cancelled while they wait
// for their remote results, and the worker reports `remote_cancel` for each.

class Forecast {
    city: string,
    source: string,
    degrees: int,
}

function remote_forecast(city: string, source: string) -> Forecast {
    Forecast { city: city, source: source, degrees: 21 }
}

function durable_forecast_race(city: string) -> Forecast {
    let fast = spawn { remote_forecast(city, "fast") };
    let medium = spawn { remote_forecast(city, "medium") };
    let slow = spawn { remote_forecast(city, "slow") };
    let winner = await baml.future.race([fast, medium, slow]);
    // The losers end at their cancelled waits before the run completes.
    baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
    winner
}

// Phase 3, part 2: durable sleep and the worker side of remote cancellation.
// Arguments and results are class values with an enum, a nested class, an
// array, a map and an optional field.

enum Cabin {
    Economy
    Business
}

class QuoteRequest {
    city: string,
    vendor: string,
    cabin: Cabin,
    nights: int,
}

class Money {
    amount: int,
    currency: string,
}

class Quote {
    vendor: string,
    cabin: Cabin,
    total: Money,
    perks: string[],
    extras: map<string, int>,
    note: string?,
}

class TripReport {
    city: string,
    quotes: Quote[],
    cheapest: Quote?,
    by_vendor: map<string, Quote>,
    cabins: Cabin[],
}

function remote_get_quote(request: QuoteRequest) -> Quote {
    Quote {
        vendor: request.vendor,
        cabin: request.cabin,
        total: Money { amount: request.nights * 100, currency: "EUR" },
        perks: [],
        extras: {},
        note: null,
    }
}

function quote_request(city: string, vendor: string, nights: int) -> QuoteRequest {
    let cabin: Cabin = Cabin.Economy;
    if (nights > 2) {
        cabin = Cabin.Business;
    }
    QuoteRequest { city: city, vendor: vendor, cabin: cabin, nights: nights }
}

function durable_nap(ms: int) -> string {
    baml.io.println("going to sleep");
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms));
    baml.io.println("woke up");
    "slept"
}

// Not durable by its name: it never suspends itself.
function plain_nap(ms: int) -> string {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms));
    "slept"
}

function durable_fan_out(city: string, ms: int) -> TripReport {
    let fs = [
        spawn { remote_get_quote(quote_request(city, "alfa", 1)) },
        spawn { remote_get_quote(quote_request(city, "bravo", 2)) },
        spawn { remote_get_quote(quote_request(city, "charlie", 3)) },
        spawn { remote_get_quote(quote_request(city, "delta", 4)) },
    ];
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms));
    let quotes = await baml.future.all(fs);
    let cheapest: Quote? = null;
    let by_vendor: map<string, Quote> = {};
    let cabins: Cabin[] = [];
    for (let q in quotes) {
        by_vendor.set(q.vendor, q);
        cabins.push(q.cabin);
        if (cheapest == null) {
            cheapest = q;
        } else if (q.total.amount < cheapest.total.amount) {
            cheapest = q;
        }
    }
    TripReport { city: city, quotes: quotes, cheapest: cheapest, by_vendor: by_vendor, cabins: cabins }
}

// A race over three remote calls, followed by a sleep of `tail_ms`. The two
// losers are cancelled in their remote waits before the sleep starts.
function durable_quote_race(city: string, tail_ms: int) -> Quote {
    let fast = spawn { remote_get_quote(quote_request(city, "fast", 1)) };
    let medium = spawn { remote_get_quote(quote_request(city, "medium", 2)) };
    let slow = spawn { remote_get_quote(quote_request(city, "slow", 3)) };
    let winner = await baml.future.race([fast, medium, slow]);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(tail_ms));
    winner
}

// A deadline around a remote call that never answers in time.
function durable_deadline(city: string, limit_ms: int) -> string {
    let answer = baml.future.with_timeout(baml.time.Duration.from_milliseconds(limit_ms), () -> {
        remote_get_quote(quote_request(city, "sluggish", 3)).vendor
    }) catch (e) {
        let timeout: baml.errors.Timeout => "no quote for " + city + ": " + timeout.message,
    };
    answer
}

// `Future.cancel` on a thread that waits for a remote result.
function durable_cancel_future(city: string) -> string {
    let pending = spawn { remote_get_quote(quote_request(city, "unwanted", 2)).vendor };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(300n));
    let did_cancel = pending.cancel();
    let outcome = (await pending) catch (e) {
        baml.panics.Cancelled => "cancelled"
    };
    outcome + " " + did_cancel.to_string()
}

class QuoteUnavailable {
    vendor: string,
    code: int,
}

// A parent thread fails while its child waits for a remote result.
function durable_failing_parent(city: string) -> string {
    let parent = spawn {
        let child = spawn { remote_get_quote(quote_request(city, "orphan", 2)).vendor };
        baml.sys.sleep(baml.time.Duration.from_milliseconds(300n));
        if (city.length() > 0) {
            throw QuoteUnavailable { vendor: "orphan", code: 503 };
        }
        await child
    };
    let outcome = (await parent) catch (e) {
        let unavailable: QuoteUnavailable => unavailable.vendor + " failed with " + unavailable.code.to_string()
    };
    // The child ends at its cancelled wait before the run completes.
    baml.sys.sleep(baml.time.Duration.from_milliseconds(300n));
    outcome
}

// The root fails while its child waits for a remote result. The run ends, so
// the worker reports the call as abandoned before the terminal event.
function durable_failing_root(city: string) -> string {
    let child = spawn { remote_get_quote(quote_request(city, "orphan", 2)).vendor };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(300n));
    if (city.length() > 0) {
        throw QuoteUnavailable { vendor: "root", code: 500 };
    }
    await child
}

// A sleep that holds an open file cannot be written to a snapshot; the next
// sleep, without the file, can.
function read_slowly(path: string, ms: int) -> string {
    let file = baml.fs.open(path, "r");
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms));
    file.text()
}

function durable_hold_file(path: string, first_ms: int, second_ms: int) -> string {
    let text = read_slowly(path, first_ms);
    baml.io.println("read " + text);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(second_ms));
    text
}

// Phase 3 review: a thread that sleeps in the background keeps the run
// suspended for a long time, while the root only needs one remote result.
function durable_background(city: string, ms: int) -> Quote {
    let background = spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(ms));
        1
    };
    let pending = spawn { remote_get_quote(quote_request(city, "solo", 3)) };
    await pending
}

// Phase 4, contract section 10.3: the call is made in a helper, so the caller
// that `remote_call` names is `fetch_via`, not the run's entry function.
function fetch_via(city: string) -> string {
    remote_fetch_weather(city + " via helper")
}

function durable_nested_weather(city: string) -> string {
    fetch_via(city)
}
