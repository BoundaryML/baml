/**
 * The phase 3 demo functions that the fixtures refer to (contract section
 * 9.4): classes, enums, maps, and nested values as arguments and results,
 * several threads, futures, a durable sleep, and remote cancellation. This is
 * a copy of `demo_durable/program/baml_src/quotes.baml`, which the site servers
 * run. It serves the source view in fixture mode, and the fixtures look their
 * line numbers up in it.
 */
export const QUOTES_BAML_FILE = "baml_src/quotes.baml";

export const QUOTES_BAML = `// Phase 3 demo functions: remote calls that take and return class values,
// fan-out with a durable sleep, race, all_settled, a deadline, and a nap.
//
// Under a plain \`baml run\` the \`durable\` and \`remote_\` markers have no effect.
// Under \`baml-cli worker\` every \`remote_get_quote\` call runs as its own run on
// a site of the remote pool, and a \`sleep\` of five seconds or more in a durable
// function suspends the run: the worker process ends, and the site server
// resumes the run from its snapshot when the sleep is over.

enum QuoteKind {
    Flight,
    Hotel,
    Car,
    Tour,
}

class Traveler {
    name: string,
    loyalty_tier: int?,
}

class QuoteRequest {
    city: string,
    kind: QuoteKind,
    nights: int,
    /// How long the vendor takes to answer.
    delay_ms: int,
    traveler: Traveler,
    /// Free-form options. \`"simulate": "unavailable"\` makes the vendor refuse.
    options: map<string, string>,
}

class Money {
    amount: int,
    currency: string,
}

class Quote {
    /// The request that this quote answers, so a result carries nested values.
    request: QuoteRequest,
    vendor: string,
    price: Money,
    tags: string[],
    /// Price of each extra, by name.
    extras: map<string, int>,
    note: string?,
}

/// The typed error of \`remote_get_quote\`.
class QuoteUnavailable {
    kind: QuoteKind,
    city: string,
    reason: string,
}

class TripReport {
    city: string,
    quotes: Quote[],
    total: Money,
    /// The vendor of each quote, by kind name.
    vendors: map<string, string>,
    cheapest: Quote?,
}

class FailedQuote {
    kind: QuoteKind,
    error: string,
}

class SettledReport {
    city: string,
    succeeded: Quote[],
    failed: FailedQuote[],
}

/// What the demo vendors offer. The functions are static, so that the only
/// top-level functions of this file are the ones that a run can start.
class Catalog {
    function kind_name(kind: QuoteKind) -> string {
        match (kind) {
            QuoteKind.Flight => "Flight",
            QuoteKind.Hotel => "Hotel",
            QuoteKind.Car => "Car",
            QuoteKind.Tour => "Tour",
        }
    }

    function nightly_rate(kind: QuoteKind) -> int {
        match (kind) {
            QuoteKind.Flight => 420,
            QuoteKind.Hotel => 135,
            QuoteKind.Car => 48,
            QuoteKind.Tour => 75,
        }
    }

    function vendor_of(kind: QuoteKind) -> string {
        match (kind) {
            QuoteKind.Flight => "Skyways",
            QuoteKind.Hotel => "Casa Azul",
            QuoteKind.Car => "Rodas",
            QuoteKind.Tour => "Seven Hills Walks",
        }
    }

    /// The request that the demo functions send for one kind of quote.
    function request(city: string, kind: QuoteKind, delay_ms: int) -> QuoteRequest {
        QuoteRequest {
            city: city,
            kind: kind,
            nights: 3,
            delay_ms: delay_ms,
            traveler: Traveler { name: "Ada", loyalty_tier: 2 },
            options: { "currency": "EUR" },
        }
    }
}

function remote_get_quote(request: QuoteRequest) -> Quote throws QuoteUnavailable | baml.errors.Io {
    let kind = Catalog.kind_name(request.kind);
    baml.io.println(
        "[remote] "
            + kind
            + " quote for "
            + request.city
            + " ("
            + request.delay_ms.to_string()
            + " ms)",
    );
    baml.sys.sleep(baml.time.Duration.from_milliseconds(request.delay_ms));
    if (request.options.get("simulate") == "unavailable") {
        throw QuoteUnavailable { kind: request.kind, city: request.city, reason: "no " + kind + " vendor answers in " + request.city };
    }
    // A flight is priced once. Everything else is priced per night.
    let amount = match (request.kind) {
        QuoteKind.Flight => Catalog.nightly_rate(request.kind),
        _ => Catalog.nightly_rate(request.kind) * request.nights,
    };
    let note: string? = null;
    if (request.traveler.loyalty_tier != null) {
        note
            = "loyalty tier "
                + (request.traveler.loyalty_tier ?? 0).to_string()
                + " applied for "
                + request.traveler.name;
    }
    Quote {
        request: request,
        vendor: Catalog.vendor_of(request.kind),
        price: Money { amount: amount, currency: request.options.get("currency") ?? "USD" },
        tags: [kind.to_lower_case(), request.city.to_lower_case()],
        extras: { "insurance": 12, "late_checkout": 30 },
        note: note,
    }
}

function durable_fan_out(city: string) -> TripReport {
    baml.io.println("asking four vendors for " + city);
    let flight = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Flight, 3000)) };
    let hotel = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Hotel, 5000)) };
    let car = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Car, 2000)) };
    let tour = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Tour, 4000)) };
    baml.io.println("sleeping 12 seconds while the vendors work");
    baml.sys.sleep(baml.time.Duration.from_seconds(12n));
    baml.io.println("awake again, collecting the quotes");
    let quotes = await baml.future.all([flight, hotel, car, tour]);
    let total = 0;
    let vendors: map<string, string> = {};
    let cheapest: Quote? = null;
    for (let q in quotes) {
        total += q.price.amount;
        vendors.set(Catalog.kind_name(q.request.kind), q.vendor);
        if (cheapest == null) {
            cheapest = q;
        } else if (q.price.amount < cheapest.price.amount) {
            cheapest = q;
        }
    }
    TripReport {
        city: city,
        quotes: quotes,
        total: Money { amount: total, currency: "EUR" },
        vendors: vendors,
        cheapest: cheapest,
    }
}

function durable_race(city: string) -> Quote {
    baml.io.println("racing three hotel vendors for " + city);
    let fast = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Hotel, 2000)) };
    let medium = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Hotel, 6000)) };
    let slow = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Hotel, 9000)) };
    let winner = await baml.future.race([fast, medium, slow]);
    baml.io.println("the winner answered after " + winner.request.delay_ms.to_string() + " ms");
    winner
}

function durable_settled(city: string) -> SettledReport {
    baml.io.println("asking three vendors for " + city + ", one of them will refuse");
    let car_request = Catalog.request(city, QuoteKind.Car, 3000);
    car_request.options.set("simulate", "unavailable");
    let kinds = [QuoteKind.Flight, QuoteKind.Car, QuoteKind.Tour];
    let flight = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Flight, 2000)) };
    let car = spawn { remote_get_quote(car_request) };
    let tour = spawn { remote_get_quote(Catalog.request(city, QuoteKind.Tour, 4000)) };
    let outcomes = await baml.future.all_settled([flight, car, tour]);
    let report = SettledReport { city: city, succeeded: [], failed: [] };
    let i = 0;
    for (let outcome in outcomes) {
        match (outcome) {
            let ok: baml.future.Success<Quote> => {
                report.succeeded.push(ok.value);
            },
            let failure: baml.future.Failure<QuoteUnavailable | baml.errors.Io> => {
                // Under a plain \`baml run\` the vendor's own error arrives. Under the
                // worker the call ran on another site, and its failure arrives as
                // \`baml.errors.Io\` with the text of the remote error.
                let message = match (failure.error) {
                    let unavailable: QuoteUnavailable => unavailable.reason,
                    let io: baml.errors.Io => io.message,
                };
                report.failed.push(FailedQuote { kind: kinds[i], error: message });
            },
            let panicked: baml.future.Panicked => {
                report.failed.push(FailedQuote { kind: kinds[i], error: \`\${panicked.panic}\` });
            },
        }
        i += 1;
    }
    baml.io.println(
        report.succeeded.length().to_string()
            + " quotes, "
            + report.failed.length().to_string()
            + " refused",
    );
    report
}

function durable_deadline(city: string) -> string {
    baml.io.println("asking a slow vendor for " + city + " with a 2 second deadline");
    let answer = baml.future.with_timeout(baml.time.Duration.from_seconds(2n), () -> {
        remote_get_quote(Catalog.request(city, QuoteKind.Tour, 6000)).vendor
    }) catch (e) {
        let timeout: baml.errors.Timeout => "no tour quote for " + city + ": " + timeout.message,
        let unavailable: QuoteUnavailable => "no tour quote for " + city + ": " + unavailable.reason,
    };
    baml.io.println(answer);
    answer
}

function durable_nap(seconds: int) -> string {
    baml.io.println("going to sleep for " + seconds.to_string() + " seconds");
    baml.sys.sleep(baml.time.Duration.from_seconds(seconds));
    baml.io.println("woke up");
    "slept " + seconds.to_string() + " seconds"
}
`;
