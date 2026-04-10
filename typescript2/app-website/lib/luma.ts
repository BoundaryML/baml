import { env } from './env';

export interface LumaEvent {
  api_id: string;
  calendar_api_id: string;
  created_at: string;
  cover_url?: string;
  name: string;
  description?: string;
  description_md?: string;
  start_at: string;
  duration_interval: string;
  end_at: string;
  geo_address_json?: string | null;
  geo_latitude?: number;
  geo_longitude?: number;
  url: string;
  timezone: string;
  user_api_id: string;
  visibility: string;
  meeting_url?: string;
  zoom_meeting_url?: string;
}

interface LumaApiEntry {
  api_id: string;
  event: LumaEvent;
  tags: string[];
}

interface LumaApiResponse {
  entries: LumaApiEntry[];
}

export async function getNextEvent(): Promise<LumaEvent | null> {
  try {
    // Get the Luma API key from environment variables
    const lumaApiKey = env.LUMA_API_KEY;

    if (!lumaApiKey) {
      console.warn('LUMA_API_KEY not found in environment variables');
      return null;
    }

    // Fetch events from Luma API with caching
    const response = await fetch(
      'https://public-api.luma.com/v1/calendar/list-events?sort_direction=desc',
      {
        headers: {
          'Content-Type': 'application/json',
          'x-luma-api-key': lumaApiKey,
        },
        method: 'GET',
        // Cache for 60 minutes
        next: { revalidate: 3600 },
      },
    );

    if (!response.ok) {
      console.error('Luma API error:', response.status, response.statusText);
      return null;
    }

    const data: LumaApiResponse = await response.json();

    // Find the next upcoming event
    const upcomingEvents = data.entries;

    return upcomingEvents?.[0]?.event || null;
  } catch (error) {
    console.error('Error fetching next event from Luma API:', error);
    return null;
  }
}
