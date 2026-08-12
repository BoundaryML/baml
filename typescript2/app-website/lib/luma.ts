import { env } from './env';

const LUMA_API_BASE = 'https://public-api.luma.com';

// Events are pulled straight from the Luma calendar; we treat an event as an
// EAP onboarding session when its name contains any of these tokens
// (case-insensitive). Luma names them "BAML Early Access", older ones "EAP".
const EAP_NAME_MATCHES = ['early access', 'eap'];

export interface LumaEvent {
  id: string;
  name: string;
  description?: string;
  description_md?: string;
  start_at: string;
  end_at: string;
  duration_interval?: string;
  timezone: string;
  url: string;
  cover_url?: string;
  // 'enabled' once the event is full and Luma opens a waitlist.
  waitlist_status?: 'enabled' | 'disabled';
  registration_open?: boolean;
}

// One of the event's registration questions, used to render our own signup
// form. `id` is what Luma expects back in registration_answers.
export interface RegistrationQuestion {
  id: string;
  label: string;
  required: boolean;
  question_type: string;
  options?: { label: string }[];
}

// An EAP event enriched with the availability signals Luma exposes and the
// registration questions so we can register people without a hop to Luma. Note:
// the public event API does NOT return a capacity, so there is no true "percent
// full" -- `goingCount` is the number approved, and `waitlist_status` /
// `registration_open` are the signals for whether an event is effectively full.
export interface EapEvent extends LumaEvent {
  goingCount: number | null;
  registrationQuestions: RegistrationQuestion[];
}

interface LumaListResponse {
  entries?: LumaEvent[];
  has_more?: boolean;
  next_cursor?: string;
}

interface LumaGuestCounts {
  approved?: { guests?: number; tickets?: number };
}

async function lumaFetch(path: string): Promise<Response | null> {
  const lumaApiKey = env.LUMA_API_KEY;
  if (!lumaApiKey) {
    console.warn('LUMA_API_KEY not found in environment variables');
    return null;
  }

  try {
    const response = await fetch(`${LUMA_API_BASE}${path}`, {
      headers: {
        accept: 'application/json',
        'x-luma-api-key': lumaApiKey,
      },
      method: 'GET',
      // Cache for 60 minutes so we don't hammer the API on every render.
      next: { revalidate: 3600 },
      // Hard timeout so a stalled connection can't hang a build/render.
      signal: AbortSignal.timeout(8000),
    });

    if (!response.ok) {
      console.error(
        'Luma API error:',
        path,
        response.status,
        response.statusText,
      );
      return null;
    }

    return response;
  } catch (error) {
    console.error('Error calling Luma API:', path, error);
    return null;
  }
}

function isEapEvent(event: LumaEvent): boolean {
  const name = event.name?.toLowerCase() ?? '';
  return EAP_NAME_MATCHES.some((token) => name.includes(token));
}

// Per-event details we can't get from the list endpoint: the approved guest
// count and the registration questions. Both come from one Get Event call.
async function getEventDetails(eventId: string): Promise<{
  goingCount: number | null;
  registrationQuestions: RegistrationQuestion[];
}> {
  const response = await lumaFetch(
    `/v1/events/get?event_id=${encodeURIComponent(eventId)}`,
  );
  if (!response) {
    return { goingCount: null, registrationQuestions: [] };
  }

  const data = (await response.json()) as {
    guest_counts?: LumaGuestCounts;
    registration_questions?: RegistrationQuestion[];
  };
  return {
    goingCount: data.guest_counts?.approved?.guests ?? null,
    registrationQuestions: data.registration_questions ?? [],
  };
}

// Upcoming (and currently-live) EAP onboarding sessions, soonest first, enriched
// with how many people are going so the page can show availability. We include
// sessions that started in the last few hours so an in-progress one still shows
// up; the client decides live-vs-upcoming with a fresh clock.
export async function getEapEvents(): Promise<EapEvent[]> {
  // Floor `after` to the hour so the request URL is stable within the 1h cache
  // window (a per-millisecond value would bust the fetch cache every render).
  const hourMs = 3_600_000;
  const windowStart = new Date(
    Math.floor(Date.now() / hourMs) * hourMs - 6 * hourMs,
  ).toISOString();
  const params = new URLSearchParams({
    after: windowStart,
    sort_column: 'start_at',
    sort_direction: 'asc',
  });

  const response = await lumaFetch(`/v1/calendars/events/list?${params}`);
  if (!response) {
    return [];
  }

  const data = (await response.json()) as LumaListResponse;
  const eapEvents = (data.entries ?? []).filter(isEapEvent);

  return Promise.all(
    eapEvents.map(async (event) => ({
      ...event,
      ...(await getEventDetails(event.id)),
    })),
  );
}

export interface RegistrationAnswer {
  question_id: string;
  value: string | boolean | string[];
}

export interface RegisterResult {
  error?: string;
  ok: boolean;
}

// Register (and auto-approve) a guest for an event via Luma, so people never
// leave our site. Luma still sends the confirmation email + Zoom link + calendar
// invite (send_email defaults to true). Server-only: uses the secret API key.
export async function registerForEvent(params: {
  answers: RegistrationAnswer[];
  email: string;
  eventId: string;
  name?: string;
}): Promise<RegisterResult> {
  const lumaApiKey = env.LUMA_API_KEY;
  if (!lumaApiKey) {
    return { error: 'Registration is temporarily unavailable.', ok: false };
  }

  try {
    const response = await fetch(`${LUMA_API_BASE}/v1/events/guests/add`, {
      body: JSON.stringify({
        approval_status: 'approved',
        event_id: params.eventId,
        guests: [
          {
            email: params.email,
            name: params.name || undefined,
            registration_answers: params.answers,
          },
        ],
        send_email: true,
      }),
      headers: {
        accept: 'application/json',
        'content-type': 'application/json',
        'x-luma-api-key': lumaApiKey,
      },
      method: 'POST',
      signal: AbortSignal.timeout(10_000),
    });

    if (!response.ok) {
      const body = await response.text();
      console.error(
        'Luma guests/add error:',
        response.status,
        body.slice(0, 300),
      );
      return {
        error: 'We could not complete your registration. Please try again.',
        ok: false,
      };
    }

    return { ok: true };
  } catch (error) {
    console.error('Luma guests/add failed:', error);
    return {
      error: 'We could not reach the registration service. Please try again.',
      ok: false,
    };
  }
}

// Most recent event on the calendar. Kept for the dormant homepage hero plumbing.
export async function getNextEvent(): Promise<LumaEvent | null> {
  const params = new URLSearchParams({
    sort_column: 'start_at',
    sort_direction: 'desc',
  });

  const response = await lumaFetch(`/v1/calendars/events/list?${params}`);
  if (!response) {
    return null;
  }

  const data = (await response.json()) as LumaListResponse;
  return data.entries?.[0] ?? null;
}
