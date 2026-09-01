import { NextResponse } from 'next/server';

export const revalidate = 86_400;

export async function GET() {
  const response = await fetch('https://api.github.com/repos/BoundaryML/baml', {
    headers: { Accept: 'application/vnd.github+json' },
    next: { revalidate },
  });

  if (!response.ok) {
    return NextResponse.json({ error: 'GitHub stars are unavailable.' }, { status: 502 });
  }

  const data = await response.json() as { stargazers_count?: unknown };
  if (typeof data.stargazers_count !== 'number') {
    return NextResponse.json({ error: 'GitHub returned an invalid star count.' }, { status: 502 });
  }

  return NextResponse.json({ stars: data.stargazers_count });
}
