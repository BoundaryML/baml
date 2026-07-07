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

// An EAP event enriched with the availability signals Luma exposes. Note: the
// public API does NOT return a capacity, so there is no true "percent full" --
// `goingCount` is the number approved, and `waitlist_status` / `registration_open`
// are the only signals for whether an event is effectively full.
export interface EapEvent extends LumaEvent {
  goingCount: number | null;
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

// Number of approved guests for an event. Requires a per-event call because the
// list endpoint does not include guest counts. Returns null if unavailable.
async function getGoingCount(eventId: string): Promise<number | null> {
  const response = await lumaFetch(
    `/v1/events/get?event_id=${encodeURIComponent(eventId)}`,
  );
  if (!response) {
    return null;
  }

  const data = (await response.json()) as { guest_counts?: LumaGuestCounts };
  return data.guest_counts?.approved?.guests ?? null;
}

// All upcoming EAP onboarding sessions, soonest first, enriched with how many
// people are going so the page can show availability.
export async function getEapEvents(): Promise<EapEvent[]> {
  const params = new URLSearchParams({
    after: new Date().toISOString(),
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
      goingCount: await getGoingCount(event.id),
    })),
  );
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
