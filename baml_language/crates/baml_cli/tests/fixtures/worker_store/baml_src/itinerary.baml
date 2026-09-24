// Fixture of `worker_program_store_e2e.rs`. The run holds classes, nested
// classes, an enum, a map, an optional, a union, and a cyclic object graph
// while it is paused, and it is resumed from the program store by a worker
// that has no project directory.

enum Season {
    Summer
    Winter
}

class Stop {
    name: string,
    nights: int,
    note: string?,
}

class Leg {
    origin: Stop,
    target: Stop,
    hours: float,
}

class Node {
    label: string,
    next: Node?,
}

class Itinerary {
    city: string,
    season: Season,
    stops: Stop[],
    legs: Leg[],
    budget: map<string, int>,
    choice: int | string,
    ring: string,
}

function season_note(season: Season) -> string? {
    match (season) {
        Season.Summer => "bring water",
        Season.Winter => null,
    }
}

// Follows `next` for `steps` nodes. On a cycle of two nodes, three steps end
// on the node they started from.
function walk_ring(start: Node, steps: int) -> string {
    let labels = start.label;
    let current: Node? = start.next;
    let taken = 1;
    while (taken < steps) {
        match (current) {
            null => { taken = steps; },
            let node: Node => {
                labels = labels + ">" + node.label;
                current = node.next;
                taken += 1;
            },
        }
    }
    labels
}

function durable_itinerary(city: string, season: Season) -> Itinerary {
    let stops: Stop[] = [];
    let legs: Leg[] = [];
    let budget: map<string, int> = { "base": 100 };
    let choice: int | string = if (season == Season.Summer) { "beach" } else { 7 };

    // A cycle: first -> second -> first.
    let first = Node { label: "first", next: null };
    let second = Node { label: "second", next: first };
    first.next = second;

    let day = 1;
    while (day < 4) {
        baml.io.println("stop " + day.to_string());
        baml.sys.sleep(baml.time.Duration.from_milliseconds(700n));
        let stop = Stop {
            name: city + " " + day.to_string(),
            nights: day,
            note: season_note(season),
        };
        if (stops.length() > 0) {
            legs.push(Leg { origin: stops[stops.length() - 1], target: stop, hours: 1.5 });
        }
        stops.push(stop);
        budget["day" + day.to_string()] = day * 10;
        day += 1
    }

    let ring = walk_ring(first, 3);
    Itinerary {
        city: city,
        season: season,
        stops: stops,
        legs: legs,
        budget: budget,
        choice: choice,
        ring: ring,
    }
}

function quick(city: string) -> Stop {
    Stop { name: city, nights: 1, note: null }
}

// A run with several threads, remote calls, and a sleep that is long enough
// for the run to suspend itself. `worker_program_store_e2e.rs` resumes it from
// the program store without a project directory.

class Offer {
    stop: Stop,
    season: Season,
    prices: map<string, int>,
    choice: int | string,
}

class OfferBoard {
    city: string,
    offers: Offer[],
    fastest: Offer,
    ring: string,
}

function remote_offer(stop: Stop, season: Season) -> Offer {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
    Offer {
        stop: stop,
        season: season,
        prices: { "night": stop.nights * 10 },
        choice: if (season == Season.Summer) { "beach" } else { 7 },
    }
}

function durable_offer_board(city: string, season: Season, nap_ms: int) -> OfferBoard {
    // A cycle that stays alive across the suspend: first -> second -> first.
    let first = Node { label: "first", next: null };
    let second = Node { label: "second", next: first };
    first.next = second;

    let one = spawn { remote_offer(Stop { name: city + " 1", nights: 1, note: season_note(season) }, season) };
    let two = spawn { remote_offer(Stop { name: city + " 2", nights: 2, note: season_note(season) }, season) };
    let three = spawn { remote_offer(Stop { name: city + " 3", nights: 3, note: season_note(season) }, season) };
    baml.io.println("napping while three offers are out");
    baml.sys.sleep(baml.time.Duration.from_milliseconds(nap_ms));
    baml.io.println("collecting the offers");
    let offers = await baml.future.all([one, two, three]);

    let quick_offer = spawn { remote_offer(Stop { name: city + " quick", nights: 1, note: null }, season) };
    let slow_offer = spawn { remote_offer(Stop { name: city + " slow", nights: 9, note: null }, season) };
    let fastest = await baml.future.race([quick_offer, slow_offer]);
    OfferBoard { city: city, offers: offers, fastest: fastest, ring: walk_ring(first, 3) }
}
