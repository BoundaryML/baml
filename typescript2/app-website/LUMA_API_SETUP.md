# Luma API Setup for Next Episode Link

This feature integrates with the Luma API to dynamically show the next upcoming BAML podcast session in the hero section.

## Setup Instructions

### 1. Get a Luma API Key

1. Navigate to your [Luma dashboard](https://lu.ma/dashboard)
2. Go to API settings
3. Generate a new API key
4. Copy the API key

### 2. Add Environment Variable

Add the following environment variable to your deployment:

```bash
LUMA_API_KEY=your_luma_api_key_here
```

### 3. For Local Development

Create a `.env.local` file in the root directory:

```bash
LUMA_API_KEY=your_luma_api_key_here
```

### 4. For Production (Vercel)

1. Go to your Vercel project dashboard
2. Navigate to Settings > Environment Variables
3. Add `LUMA_API_KEY` with your Luma API key value

## How It Works

The feature uses **server-side data fetching** for optimal performance:

1. **Main page** (`app/page.tsx`) fetches the next event server-side during build/render time
2. **60-minute caching** is applied using Next.js `revalidate: 3600`
3. **Data is passed down** to the HeroSection component as props
4. **Client component** receives the data and updates the "Try BAML online" button to show:
   - "🔴 Live Now - Join BAML Session" if an event is currently live
   - "✨ Next BAML Session: [Date]" if there's an upcoming event
   - "✨ Try BAML online" as fallback

## Caching Strategy

- **Server-side caching**: 60 minutes with `revalidate: 3600`
- **Build-time optimization**: Data is fetched during page generation
- **No client-side API calls**: Data is available immediately on page load
- **Error resilience**: Falls back gracefully if API is unavailable

## API Endpoints Used

- **List Events**: `GET /v1/calendars/{calendar_id}/events`
- **Authentication**: `x-luma-api-key` header

## Files Modified

- `lib/luma.ts` - Utility function to fetch next event with caching
- `app/page.tsx` - Main page that fetches data server-side
- `components/landing/hero-section.tsx` - Updated to accept nextEvent prop
- `components/landing/next-episode-link.tsx` - Updated to use server-side data

## Performance Benefits

- **Zero client-side API calls**: Data is fetched server-side
- **Instant page loads**: No loading states for returning visitors
- **Better SEO**: Data is available during server-side rendering
- **Cost effective**: Fewer API calls to Luma
- **Simpler architecture**: No separate API route needed

## Fallback Behavior

If the Luma API is unavailable or no upcoming events are found, the button will default to linking to `/playground`.