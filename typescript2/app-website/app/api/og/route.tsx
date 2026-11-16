import { ImageResponse } from 'next/og';
import type { NextRequest } from 'next/server';
import { getPost } from '@/app/(home)/blog/_lib/get-posts';
import {
  boundaryLogo,
  duskField,
  ogFonts,
  podcastHosts,
  teamPhoto,
} from '@/components/og/og-assets';
import { OG_SIZE, OgCard } from '@/components/og/og-card';

export const runtime = 'nodejs';

// Pick a headline size that keeps long titles inside the card.
function titleFontSize(title: string): number {
  const n = title.length;
  if (n > 62) return 44;
  if (n > 42) return 52;
  if (n > 24) return 58;
  return 72;
}

function clamp(text: string, max: number): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max - 1).trimEnd()}…`;
}

export async function GET(request: NextRequest) {
  const params = request.nextUrl.searchParams;
  const slug = params.get('slug');

  let title = params.get('title') || 'The programming language for agents';
  let description = params.get('desc') || '';
  let footer = params.get('footer') || 'boundaryml.com';
  const avatars = params.get('podcast') === '1' ? podcastHosts() : undefined;
  const photo = params.get('team') === '1' ? teamPhoto() : undefined;

  // Blog posts derive their card from the post's frontmatter.
  if (slug) {
    const post = await getPost(slug);
    if (post) {
      title = post.title;
      description = post.description || '';
      footer = post.author?.name
        ? `${post.author.name} · boundaryml.com`
        : 'boundaryml.com';
    } else {
      title = 'Post not found';
      description = '';
    }
  }

  return new ImageResponse(
    OgCard({
      avatars,
      description: clamp(description, 140),
      footer,
      logo: boundaryLogo(),
      painting: duskField(),
      photo,
      title: clamp(title, 90),
      titleFontSize: titleFontSize(title),
    }),
    { ...OG_SIZE, fonts: ogFonts() },
  );
}
