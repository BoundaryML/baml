import {
  customCtx,
  customMutation,
} from 'convex-helpers/server/customFunctions';
import { Triggers } from 'convex-helpers/server/triggers';

import { internal } from './_generated/api';
import type { DataModel } from './_generated/dataModel';
import {
  internalMutation as rawInternalMutation,
  mutation as rawMutation,
} from './_generated/server';

// Reactive onboarding. Convex has no native "on insert" server hook, so we use
// convex-helpers Triggers: any write to `councilSubmissions` runs the callback
// below. We react to the data instead of coupling onboarding into the form —
// the website's `council.submit` stays a plain `db.insert` and knows nothing
// about Loops.
//
// FOOTGUN: every mutation that writes to the DB MUST build itself from the
// `mutation` / `internalMutation` exported here — NOT from `./_generated/server`.
// A mutation built from the raw server module bypasses these triggers, so its
// inserts never get onboarded to Loops.
const triggers = new Triggers<DataModel>();

triggers.register('councilSubmissions', async (ctx, change) => {
  if (change.operation === 'insert') {
    // Fire-and-forget: schedule the side-effect actions so the mutation stays
    // fast and the HTTP calls happen outside the transaction. Both are
    // idempotent (each no-ops if its row is already synced).
    await ctx.scheduler.runAfter(0, internal.loops.onboard, {
      submissionId: change.id,
    });
    await ctx.scheduler.runAfter(0, internal.discord.assignRole, {
      submissionId: change.id,
    });
  }
});

export const mutation = customMutation(rawMutation, customCtx(triggers.wrapDB));
export const internalMutation = customMutation(
  rawInternalMutation,
  customCtx(triggers.wrapDB),
);
