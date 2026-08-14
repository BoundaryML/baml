import { v } from 'convex/values';

import { internal } from './_generated/api';
import type { Doc } from './_generated/dataModel';
import { internalAction, internalQuery } from './_generated/server';
import { internalMutation } from './triggers';

// Loops API (https://loops.so/docs/api-reference). Base URL, bearer auth, and a
// 10 req/s limit. We touch three endpoints: create a contact, update a contact
// (fallback when it already exists), and send an event (which triggers the
// welcome workflow that listens on that event name).
const LOOPS_BASE = 'https://app.loops.so/api/v1';

function loopsFetch(
  apiKey: string,
  method: string,
  path: string,
  body: Record<string, unknown>,
) {
  return fetch(`${LOOPS_BASE}${path}`, {
    body: JSON.stringify(body),
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json',
    },
    method,
  });
}

// Add the member to the council mailing list. Loops returns 409 if the email
// already exists — fall back to update so re-submits and pre-existing
// subscribers still get added to the list. Returns the Loops contact id.
async function upsertContact(
  apiKey: string,
  listId: string | undefined,
  sub: Doc<'councilSubmissions'>,
): Promise<string | undefined> {
  const payload: Record<string, unknown> = {
    email: sub.email,
    // Fall back to discord for legacy rows that predate firstName.
    firstName: sub.firstName || sub.discord || undefined,
    lastName: sub.lastName || undefined,
  };
  // The mailing list IS the "Sheep Council Member" title (per the design: title
  // is represented via a Loops list, not a custom property / userGroup).
  if (listId) {
    payload.mailingLists = { [listId]: true };
  }

  const created = await loopsFetch(apiKey, 'POST', '/contacts/create', payload);
  if (created.status === 409) {
    const updated = await loopsFetch(apiKey, 'PUT', '/contacts/update', payload);
    const body = await updated.json();
    if (!updated.ok || body?.success === false) {
      throw new Error(
        `Loops contacts/update failed: ${updated.status} ${JSON.stringify(body)}`,
      );
    }
    return body?.id;
  }

  const body = await created.json();
  if (!created.ok || body?.success === false) {
    throw new Error(
      `Loops contacts/create failed: ${created.status} ${JSON.stringify(body)}`,
    );
  }
  return body?.id;
}

// Fire the event the welcome workflow is configured to listen on.
async function sendWelcome(apiKey: string, eventName: string, email: string) {
  const res = await loopsFetch(apiKey, 'POST', '/events/send', {
    email,
    eventName,
  });
  const body = await res.json();
  if (!res.ok || body?.success === false) {
    throw new Error(
      `Loops events/send failed: ${res.status} ${JSON.stringify(body)}`,
    );
  }
}

export const getSubmission = internalQuery({
  args: { submissionId: v.id('councilSubmissions') },
  handler: (ctx, { submissionId }) => ctx.db.get(submissionId),
});

export const markSynced = internalMutation({
  args: {
    contactId: v.optional(v.string()),
    submissionId: v.id('councilSubmissions'),
  },
  handler: async (ctx, { contactId, submissionId }) => {
    await ctx.db.patch(submissionId, {
      loopsContactId: contactId,
      loopsError: undefined,
      loopsSyncedAt: Date.now(),
    });
  },
});

export const recordError = internalMutation({
  args: { error: v.string(), submissionId: v.id('councilSubmissions') },
  handler: async (ctx, { error, submissionId }) => {
    const sub = await ctx.db.get(submissionId);
    if (!sub) {
      return;
    }
    await ctx.db.patch(submissionId, {
      loopsAttempts: (sub.loopsAttempts ?? 0) + 1,
      loopsError: error,
    });
  },
});

// Onboard one submission to Loops: add to the council list, send the welcome
// event, then stamp `loopsSyncedAt`. Idempotent — it no-ops if already synced.
// Called only by the insert trigger (one row at a time); there is no sweep.
export const onboard = internalAction({
  args: { submissionId: v.id('councilSubmissions') },
  handler: async (ctx, { submissionId }) => {
    const apiKey = process.env.LOOPS_API_KEY;
    if (!apiKey) {
      throw new Error('LOOPS_API_KEY is not set');
    }
    const listId = process.env.LOOPS_COUNCIL_LIST_ID;
    const eventName = process.env.LOOPS_WELCOME_EVENT ?? 'sheep_council_welcome';

    const sub = await ctx.runQuery(internal.loops.getSubmission, {
      submissionId,
    });
    if (!sub || sub.loopsSyncedAt) {
      return; // deleted before we got here, or already onboarded
    }

    try {
      const contactId = await upsertContact(apiKey, listId, sub);
      await sendWelcome(apiKey, eventName, sub.email);
      await ctx.runMutation(internal.loops.markSynced, {
        contactId,
        submissionId,
      });
    } catch (err) {
      await ctx.runMutation(internal.loops.recordError, {
        error: err instanceof Error ? err.message : String(err),
        submissionId,
      });
      throw err; // surface in the Convex logs (no auto-retry — insert only)
    }
  },
});

