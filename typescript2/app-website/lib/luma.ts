import { env } from './env';

const LUMA_API_BASE = 'https://public-api.luma.com';

// Events are pulled straight from the Luma calendar; we treat an event as an
// EAP onboarding session when its name contains any of these tokens
// (case-insensitive). Luma names them "BAML Early Access", older ones "EAP".
const EAP_NAME_MATCHES = ['early access', 'eap', 'onboarding'];

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
  options?: (string | { label: string })[];
  multiple?: boolean | null;
  collect_job_title?: boolean | null;
  job_title_label?: string | null;
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
  entries?: (LumaEvent & {
    visibility?: 'public' | 'members-only' | 'private';
    registration_questions?: RegistrationQuestion[];
  })[];
  has_more?: boolean;
  next_cursor?: string;
}

async function lumaFetch(
  path: string,
  revalidate: number | false = 3600,
): Promise<Response | null> {
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
      ...(revalidate === false
        ? { cache: 'no-store' as const }
        : { next: { revalidate } }),
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

// The calendar list includes registration questions and availability for
// managed events. Only return fields the public signup form needs.
export async function getEapEvents(): Promise<EapEvent[]> {
  const hourMs = 3_600_000;
  // Luma's "after" filter uses the start time. Include recent starts so
  // sessions can still be listed until they end.
  const params = new URLSearchParams({
    after: new Date(
      (Math.floor(Date.now() / hourMs) - 24) * hourMs,
    ).toISOString(),
    sort_column: 'start_at',
    sort_direction: 'asc',
  });
  const sessions: EapEvent[] = [];
  while (true) {
    // The banner and picker should reflect rescheduled sessions quickly.
    const response = await lumaFetch(`/v1/calendars/events/list?${params}`, 60);
    if (!response) break;
    const data = (await response.json()) as LumaListResponse;
    for (const event of data.entries ?? []) {
      if (event.visibility !== 'public' || !isEapEvent(event)) continue;
      const session = {
        end_at: event.end_at,
        goingCount: null,
        id: event.id,
        name: event.name,
        registration_open: event.registration_open,
        registrationQuestions: (event.registration_questions ?? []).filter(
          (question) => question.question_type !== 'terms' || question.required,
        ),
        start_at: event.start_at,
        timezone: event.timezone,
        url: event.url,
        waitlist_status: event.waitlist_status,
      };
      if (canRegisterForOnboarding(session)) sessions.push(session);
    }
    if (
      !data.has_more ||
      !data.next_cursor ||
      params.get('pagination_cursor') === data.next_cursor
    )
      break;
    params.set('pagination_cursor', data.next_cursor);
  }
  return sessions;
}

export function canRegisterForOnboarding(event: EapEvent) {
  return (
    event.registration_open === true &&
    Date.parse(event.end_at) > Date.now() &&
    !event.registrationQuestions.some(
      (question) => question.required && question.question_type === 'terms',
    )
  );
}

export async function getOnboardingSession(
  eventId: string,
): Promise<(EapEvent & { meeting_url?: string | null }) | null> {
  if (!/^evt-[a-zA-Z0-9]+$/.test(eventId)) return null;
  const response = await lumaFetch(
    `/v1/events/get?event_id=${encodeURIComponent(eventId)}`,
    false,
  );
  if (!response) return null;
  const event = (await response.json()) as LumaEvent & {
    meeting_url?: string | null;
    visibility?: string;
    registration_questions?: RegistrationQuestion[];
  };
  if (event.visibility !== 'public' || !isEapEvent(event)) return null;
  const session = {
    ...event,
    goingCount: null,
    registrationQuestions: event.registration_questions ?? [],
  };
  return canRegisterForOnboarding(session) ? session : null;
}

export function zoomAttendeeUrl(meetingUrl: string | null | undefined) {
  if (!meetingUrl || !URL.canParse(meetingUrl)) return undefined;
  const url = new URL(meetingUrl);
  if (
    url.protocol === 'https:' &&
    !url.username &&
    !url.password &&
    !url.port &&
    (url.hostname === 'zoom.us' || url.hostname.endsWith('.zoom.us')) &&
    /^\/(j|w)\/\d+\/?$/.test(url.pathname)
  ) {
    return url.href;
  }
}

export interface RegistrationAnswer {
  question_id: string;
  value:
    | string
    | boolean
    | string[]
    | { company: string; job_title: string | null };
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
  sendEmail?: boolean;
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
        send_email: params.sendEmail ?? true,
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
      console.error('Luma guests/add failed:', response.status);
      return {
        error: 'We could not complete your registration. Please try again.',
        ok: false,
      };
    }

    const result = (await response.json()) as { skipped: { email: string }[] };
    if (result.skipped.length > 0) {
      return {
        error: 'Luma could not register this email for the session.',
        ok: false,
      };
    }
    return { ok: true };
  } catch {
    console.error('Luma guests/add could not be completed.');
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
