// Deterministic tests for the executors.
//
// KNOWN UPSTREAM BUG (verified through 0.13.1-nightly.20260707.c): the
// DecideAction$parse companion throws InvalidArgument "Unions must be
// flattened" for ANY union return type, even a flat two-class union — while
// the live LLM call path parses the same union correctly. Until that's fixed,
// SAP regression coverage for DecideAction lives in live smoke tests only.

testset "executors" {
  test "timer formats duration and payload" {
    let out = execute(SetTimer { seconds: 5430, label: "pasta" });
    assert.equal(out.tool, "timer");
    assert.equal(out.say, "Timer set for 1 hour and 30 minutes.");
  }

  test "timer singular units" {
    let out = execute(SetTimer { seconds: 61, label: null });
    assert.equal(out.say, "Timer set for 1 minute and 1 second.");
  }

  test "timer rejects nonpositive duration" {
    let out = execute(SetTimer { seconds: 0, label: null });
    assert.equal(out.tool, "error");
  }

  test "order lookup normalizes case" {
    let out = execute(LookupOrder { order_id: "a1b2-c3d4" });
    assert.equal(out.tool, "order");
    assert.equal(out.say, "Order A1B2-C3D4, your mechanical keyboard, is shipped with expected arrival July 9.");
  }

  test "order lookup misses politely" {
    let out = execute(LookupOrder { order_id: "NOPE-0000" });
    assert.equal(out.tool, "order");
    assert.is_true(out.say.includes("couldn't find an order"));
  }

  test "answer passes through" {
    let out = execute(Answer { message: "Ulaanbaatar." });
    assert.equal(out.tool, "answer");
    assert.equal(out.say, "Ulaanbaatar.");
  }

  test "pick_place honors region" {
    let results = [
      GeoResult { name: "Paris", latitude: 48.85, longitude: 2.35, country: "France", admin1: "Île-de-France" },
      GeoResult { name: "Paris", latitude: 33.66, longitude: -95.55, country: "United States", admin1: "Texas" },
    ];
    match (pick_place(results, "texas")) {
      null => assert.is_true(false),
      let p: GeoResult => assert.equal(p.longitude, -95.55),
    };
    match (pick_place(results, null)) {
      null => assert.is_true(false),
      let p: GeoResult => assert.equal(p.longitude, 2.35),
    };
  }
}

testset "complex-executors" {
  test "calendar speaks the datetime" {
    let out = run_calendar(CreateCalendarEvent {
      title: "standup", start_iso: "2026-07-14T09:30:00",
      duration_minutes: 15, attendees: ["alice"], location: null,
    });
    assert.equal(out.tool, "calendar");
    assert.equal(out.say, "Booked \"standup\" with alice on July 14 at 9:30 AM for 15 minutes.");
  }

  test "calendar rejects nonpositive duration" {
    let out = run_calendar(CreateCalendarEvent {
      title: "x", start_iso: "2026-07-14T09:30:00",
      duration_minutes: 0, attendees: [], location: null,
    });
    assert.equal(out.tool, "error");
  }

  test "message resolves contact with punctuation" {
    let out = run_send_message(SendMessage { to: "Dr. Chen", body: "hi", urgency: "normal" });
    assert.equal(out.tool, "message");
    assert.is_true(out.say.includes("Message sent"));
  }

  test "message misses politely" {
    let out = run_send_message(SendMessage { to: "charlie", body: "hi", urgency: "low" });
    assert.is_true(out.say.includes("don't have a contact"));
  }

  test "pm noon edge" {
    assert.equal(speakable_datetime("2026-07-14T12:05:00"), "July 14 at 12:05 PM");
    assert.equal(speakable_datetime("2026-07-14T00:05:00"), "July 14 at 12:05 AM");
  }
}
