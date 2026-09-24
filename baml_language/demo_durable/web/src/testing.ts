/** Helpers shared by the unit tests. */

import type { Fixture } from "./fixtures";
import { initialState, reducer, type AppState } from "./state";

/** Feeds every message of a fixture with `ts <= until` into the reducer. */
export function playFixture(fixture: Fixture, until = Number.POSITIVE_INFINITY, from: AppState = initialState()): AppState {
  let state = from;
  for (const event of fixture.events) {
    if (event.ts > until) break;
    state = reducer(state, { type: "sse", site: event.site, event });
  }
  return state;
}
